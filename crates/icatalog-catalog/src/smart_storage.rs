//! Normalized rules and fixed-shape SQL evaluation; user input is never SQL.
use crate::smart_expr::{invalid, MAX_BYTES, MAX_DEPTH, MAX_NODES};
use crate::{Catalog, CatalogError, Result, SmartExpr, SmartPredicate, SmartRules};
use rusqlite::functions::{Aggregate, Context, FunctionFlags};
use rusqlite::types::Value;
use rusqlite::{params, Connection};

// All aliases are trusted. A correlated aggregate evaluates a bounded rule tree
// for one asset, independent of how many Collections are selected by the caller.
pub(crate) const SMART_MATCH: &str = "(SELECT icatalog_smart_match(n.ordinal,n.parent,n.kind,n.field,n.text_value,n.int_value,
 CASE WHEN n.kind!=2 THEN 0
 WHEN length(CAST(n.text_value AS BLOB))>1024 THEN 0
 ELSE CASE n.field
 WHEN 'format' THEN icatalog_format_matches(asset.format,n.text_value)
 WHEN 'captured-from' THEN EXISTS(SELECT 1 FROM asset_meta m WHERE m.asset_id=asset.id AND substr(m.captured_local,1,10)>=n.text_value)
 WHEN 'captured-to' THEN EXISTS(SELECT 1 FROM asset_meta m WHERE m.asset_id=asset.id AND substr(m.captured_local,1,10)<=n.text_value)
 WHEN 'camera' THEN EXISTS(SELECT 1 FROM asset_meta m WHERE m.asset_id=asset.id AND icatalog_contains_ci(coalesce(m.make,'') || ' ' || coalesce(m.model,''),n.text_value))
 WHEN 'lens' THEN EXISTS(SELECT 1 FROM asset_meta m WHERE m.asset_id=asset.id AND icatalog_contains_ci(m.lens,n.text_value))
 WHEN 'min-rating' THEN coalesce((SELECT rating FROM annotation WHERE asset_id=asset.id),0)>=n.int_value
 WHEN 'max-rating' THEN coalesce((SELECT rating FROM annotation WHERE asset_id=asset.id),0)<=n.int_value
 WHEN 'label' THEN coalesce((SELECT label FROM annotation WHERE asset_id=asset.id),0)=n.int_value
 WHEN 'keyword' THEN EXISTS(SELECT 1 FROM asset_keyword ak JOIN keyword k ON k.id=ak.keyword_id WHERE ak.asset_id=asset.id AND k.name=n.text_value)
 WHEN 'text' THEN icatalog_contains_ci(asset.name,n.text_value) OR icatalog_contains_ci(asset.path,n.text_value)
 ELSE 0 END END)
 FROM smart_rule_node n WHERE n.collection_id=sc.collection_id)";

pub(crate) fn membership_sql() -> String {
    format!("(CASE WHEN sc.collection_id IS NULL THEN EXISTS (SELECT 1 FROM collection_asset ca WHERE ca.collection_id=c.id AND ca.asset_id=asset.id) ELSE {SMART_MATCH} END)")
}

#[derive(Clone)]
struct Node {
    ordinal: usize,
    parent: Option<usize>,
    kind: i64,
    predicate: Option<SmartPredicate>,
    matched: bool,
}

fn index(value: i64) -> Result<usize> {
    usize::try_from(value)
        .ok()
        .filter(|v| *v < MAX_NODES)
        .ok_or_else(invalid)
}

fn node(
    ordinal: i64,
    parent: Option<i64>,
    kind: i64,
    field: Option<String>,
    text: Option<String>,
    number: Option<i64>,
    matched: bool,
) -> Result<Node> {
    let ordinal = index(ordinal)?;
    let parent = parent.map(index).transpose()?;
    if (ordinal == 0 && (parent.is_some() || kind == 2))
        || (ordinal != 0 && parent.is_none_or(|v| v >= ordinal))
    {
        return Err(invalid());
    }
    let predicate = match kind {
        0 | 1 if field.is_none() && text.is_none() && number.is_none() => None,
        2 => Some(SmartPredicate::from_parts(
            field.as_deref().ok_or_else(invalid)?,
            text,
            number,
        )?),
        _ => return Err(invalid()),
    };
    Ok(Node {
        ordinal,
        parent,
        kind,
        predicate,
        matched,
    })
}

fn validate_nodes(nodes: &mut [Node]) -> Result<()> {
    if nodes.is_empty() || nodes.len() > MAX_NODES {
        return Err(invalid());
    }
    nodes.sort_by_key(|n| n.ordinal);
    if nodes.first().is_none_or(|n| n.ordinal != 0) {
        return Err(invalid());
    }
    let mut depths = [None; MAX_NODES];
    let mut kinds = [None; MAX_NODES];
    let mut child_counts = [0usize; MAX_NODES];
    let mut bytes = 0usize;
    for n in nodes.iter() {
        if kinds.get(n.ordinal).is_none_or(Option::is_some) {
            return Err(invalid());
        }
        let depth = if let Some(parent) = n.parent {
            if kinds.get(parent).copied().flatten().is_none_or(|v| v == 2) {
                return Err(invalid());
            }
            let depth: usize = depths.get(parent).copied().flatten().ok_or_else(invalid)?;
            if let Some(count) = child_counts.get_mut(parent) {
                *count = count.saturating_add(1);
            }
            depth.saturating_add(1)
        } else {
            0
        };
        if (n.kind != 2 && depth >= MAX_DEPTH) || depth > MAX_DEPTH {
            return Err(invalid());
        }
        *depths.get_mut(n.ordinal).ok_or_else(invalid)? = Some(depth);
        *kinds.get_mut(n.ordinal).ok_or_else(invalid)? = Some(n.kind);
        bytes = bytes.saturating_add(n.predicate.as_ref().map_or(10, SmartPredicate::json_len));
        if bytes > MAX_BYTES {
            return Err(invalid());
        }
    }
    if nodes
        .iter()
        .any(|n| n.kind != 2 && child_counts.get(n.ordinal) == Some(&0))
    {
        return Err(invalid());
    }
    for n in nodes.iter().filter(|n| n.kind != 2) {
        bytes = bytes.saturating_add(
            child_counts
                .get(n.ordinal)
                .copied()
                .unwrap_or(0)
                .saturating_sub(1),
        );
    }
    if bytes > MAX_BYTES {
        return Err(invalid());
    }
    Ok(())
}

fn flatten(expr: &SmartExpr) -> Result<Vec<Node>> {
    expr.validate()?;
    let mut nodes = Vec::new();
    let mut pending = vec![(expr, None)];
    while let Some((expr, parent)) = pending.pop() {
        let ordinal = nodes.len();
        let (kind, predicate) = match expr {
            SmartExpr::All(_) => (0, None),
            SmartExpr::Any(_) => (1, None),
            SmartExpr::Rule(p) => (2, Some(p.clone())),
        };
        nodes.push(Node {
            ordinal,
            parent,
            kind,
            predicate,
            matched: false,
        });
        if let SmartExpr::All(children) | SmartExpr::Any(children) = expr {
            pending.extend(children.iter().rev().map(|child| (child, Some(ordinal))));
        }
    }
    Ok(nodes)
}

fn expression(nodes: &mut [Node]) -> Result<SmartExpr> {
    validate_nodes(nodes)?;
    let mut built: Vec<Option<SmartExpr>> = vec![None; MAX_NODES];
    for n in nodes.iter().rev() {
        let expr = if let Some(predicate) = &n.predicate {
            SmartExpr::Rule(predicate.clone())
        } else {
            let children = nodes
                .iter()
                .filter(|child| child.parent == Some(n.ordinal))
                .map(|child| {
                    built
                        .get_mut(child.ordinal)
                        .and_then(Option::take)
                        .ok_or_else(invalid)
                })
                .collect::<Result<Vec<_>>>()?;
            if n.kind == 0 {
                SmartExpr::All(children)
            } else {
                SmartExpr::Any(children)
            }
        };
        *built.get_mut(n.ordinal).ok_or_else(invalid)? = Some(expr);
    }
    built.first_mut().and_then(Option::take).ok_or_else(invalid)
}

fn sql_error(error: CatalogError) -> rusqlite::Error {
    rusqlite::Error::UserFunctionError(Box::new(error))
}

struct Matcher;
impl Aggregate<Vec<Node>, bool> for Matcher {
    fn init(&self, _: &mut Context<'_>) -> rusqlite::Result<Vec<Node>> {
        Ok(Vec::new())
    }
    fn step(&self, ctx: &mut Context<'_>, nodes: &mut Vec<Node>) -> rusqlite::Result<()> {
        if nodes.len() >= MAX_NODES {
            return Err(sql_error(invalid()));
        }
        // Check borrowed bytes before String allocation for hostile SQLite rows.
        for (index, maximum) in [(3, 32), (4, 1024)] {
            if let rusqlite::types::ValueRef::Text(bytes) = ctx.get_raw(index) {
                if bytes.len() > maximum {
                    return Err(sql_error(invalid()));
                }
            }
        }
        nodes.push(
            node(
                ctx.get(0)?,
                ctx.get(1)?,
                ctx.get(2)?,
                ctx.get(3)?,
                ctx.get(4)?,
                ctx.get(5)?,
                ctx.get::<Option<bool>>(6)?.unwrap_or(false),
            )
            .map_err(sql_error)?,
        );
        Ok(())
    }
    fn finalize(&self, _: &mut Context<'_>, nodes: Option<Vec<Node>>) -> rusqlite::Result<bool> {
        let mut nodes = nodes.ok_or_else(|| sql_error(invalid()))?;
        validate_nodes(&mut nodes).map_err(sql_error)?;
        let mut values = [false; MAX_NODES];
        for n in nodes.iter().rev() {
            let value = match n.kind {
                0 => nodes
                    .iter()
                    .filter(|child| child.parent == Some(n.ordinal))
                    .all(|child| values.get(child.ordinal) == Some(&true)),
                1 => nodes
                    .iter()
                    .filter(|child| child.parent == Some(n.ordinal))
                    .any(|child| values.get(child.ordinal) == Some(&true)),
                _ => n.matched,
            };
            *values
                .get_mut(n.ordinal)
                .ok_or_else(|| sql_error(invalid()))? = value;
        }
        values.first().copied().ok_or_else(|| sql_error(invalid()))
    }
}

pub(crate) fn register(conn: &Connection) -> Result<()> {
    conn.create_aggregate_function(
        "icatalog_smart_match",
        7,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        Matcher,
    )?;
    Ok(())
}

/// Read old recovery backups/snapshots without migrating or altering their bytes.
/// A temporary view supplies normalized rows only for recorded v6-v8 schemas;
/// a damaged v9 node table must remain an error, never a compatibility fallback.
pub(crate) fn legacy_read_view(conn: &Connection) -> Result<()> {
    let version: i64 =
        conn.query_row("SELECT MAX(version) FROM main.schema_migrations", [], |r| {
            r.get(0)
        })?;
    if !(6..=8).contains(&version) {
        return Ok(());
    }
    let mut selects = vec!["SELECT collection_id,0 AS ordinal,NULL AS parent,0 AS kind,NULL AS field,NULL AS text_value,NULL AS int_value FROM main.smart_collection".to_owned()];
    for (ordinal, column) in [
        "format",
        "captured_from",
        "captured_to",
        "camera",
        "lens",
        "min_rating",
        "max_rating",
        "label",
        "keyword",
        "text",
    ]
    .into_iter()
    .enumerate()
    {
        if column == "max_rating" && version < 8 {
            continue;
        }
        let numeric = matches!(column, "min_rating" | "max_rating" | "label");
        let text = if numeric { "NULL" } else { column };
        let number = if numeric { column } else { "NULL" };
        selects.push(format!("SELECT collection_id,{},0,2,'{}',{text},{number} FROM main.smart_collection WHERE {column} IS NOT NULL", ordinal + 1, column.replace('_', "-")));
    }
    conn.execute_batch(&format!(
        "CREATE TEMP VIEW smart_rule_node AS {}",
        selects.join(" UNION ALL ")
    ))?;
    Ok(())
}

impl Catalog {
    pub fn count_smart_matches(&self, rules: &SmartRules) -> Result<i64> {
        let expr = rules.canonical_expr()?;
        let nodes = flatten(&expr)?;
        let mut values = Vec::<Value>::new();
        for n in &nodes {
            values.extend([
                Value::Integer(0),
                Value::Integer(n.ordinal as i64),
                n.parent
                    .map(|v| Value::Integer(v as i64))
                    .unwrap_or(Value::Null),
                Value::Integer(n.kind),
                n.predicate
                    .as_ref()
                    .map(|p| Value::Text(p.field().into()))
                    .unwrap_or(Value::Null),
                n.predicate
                    .as_ref()
                    .and_then(SmartPredicate::text)
                    .map(|v| Value::Text(v.into()))
                    .unwrap_or(Value::Null),
                n.predicate
                    .as_ref()
                    .and_then(SmartPredicate::number)
                    .map(Value::Integer)
                    .unwrap_or(Value::Null),
            ]);
        }
        let marks = vec!["(?,?,?,?,?,?,?)"; nodes.len()].join(",");
        Ok(self.conn.query_row(&format!("WITH smart_rule_node(collection_id,ordinal,parent,kind,field,text_value,int_value) AS (VALUES {marks}), sc(collection_id) AS (VALUES (0)) SELECT count(*) FROM asset CROSS JOIN sc WHERE {SMART_MATCH}"), rusqlite::params_from_iter(values), |row| row.get(0))?)
    }

    pub fn smart_rules(&self, id: i64) -> Result<Option<SmartRules>> {
        let present: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM smart_collection WHERE collection_id=?1)",
            [id],
            |row| row.get(0),
        )?;
        if !present {
            return Ok(None);
        }
        let mut stmt = self.conn.prepare("SELECT ordinal,parent,kind,CASE WHEN length(CAST(field AS BLOB))>32 THEN '' ELSE field END,CASE WHEN length(CAST(text_value AS BLOB))>1024 THEN '' ELSE text_value END,int_value FROM smart_rule_node WHERE collection_id=?1 ORDER BY ordinal LIMIT 129")?;
        let rows = stmt.query_map([id], |row| {
            node(
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                false,
            )
            .map_err(sql_error)
        })?;
        let mut nodes = Vec::new();
        for row in rows {
            if nodes.len() >= MAX_NODES {
                return Err(invalid());
            }
            nodes.push(row?);
        }
        Ok(Some(SmartRules::from_expression(expression(&mut nodes)?)?))
    }

    /// Atomically replaces one tree, including for callers without an outer transaction.
    pub fn set_smart_rules(&self, id: i64, rules: &SmartRules) -> Result<()> {
        let expr = rules.canonical_expr()?;
        if let Some(old) = self.smart_rules(id)? {
            if old.canonical_expr()? == expr {
                return Ok(());
            }
        }
        let nodes = flatten(&expr)?;
        self.conn
            .execute_batch("SAVEPOINT icatalog_set_smart_rules")?;
        let result = (|| -> Result<()> {
            self.conn.execute("INSERT INTO smart_collection(collection_id) VALUES(?1) ON CONFLICT(collection_id) DO NOTHING", [id])?;
            self.conn
                .execute("DELETE FROM smart_rule_node WHERE collection_id=?1", [id])?;
            let mut statement = self.conn.prepare("INSERT INTO smart_rule_node(collection_id,ordinal,parent,kind,field,text_value,int_value) VALUES(?1,?2,?3,?4,?5,?6,?7)")?;
            for n in nodes {
                statement.execute(params![
                    id,
                    n.ordinal as i64,
                    n.parent.map(|v| v as i64),
                    n.kind,
                    n.predicate.as_ref().map(SmartPredicate::field),
                    n.predicate.as_ref().and_then(SmartPredicate::text),
                    n.predicate.as_ref().and_then(SmartPredicate::number)
                ])?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.conn.execute_batch(
                "ROLLBACK TO icatalog_set_smart_rules; RELEASE icatalog_set_smart_rules",
            )?;
            return Err(error);
        }
        // RELEASE can fail (for example on a full disk); retain rollback ability.
        if let Err(error) = self.conn.execute_batch("RELEASE icatalog_set_smart_rules") {
            let _ = self.conn.execute_batch(
                "ROLLBACK TO icatalog_set_smart_rules; RELEASE icatalog_set_smart_rules",
            );
            return Err(error.into());
        }
        Ok(())
    }
}
