-- Grouped Boolean Smart Collection rules. Copy without changing revisions.
DROP TRIGGER smart_no_manual_members;
DROP TRIGGER smart_no_manual_member_updates;
DROP TRIGGER smart_no_existing_members;
DROP TRIGGER smart_collection_revision_insert;
DROP TRIGGER smart_collection_revision_update;
DROP TRIGGER smart_collection_revision_delete;
DROP TRIGGER smart_collection_generation_insert;
DROP TRIGGER smart_collection_generation_update;
DROP TRIGGER smart_collection_generation_delete;
ALTER TABLE smart_collection RENAME TO smart_collection_v8;
CREATE TABLE smart_collection (
  collection_id INTEGER PRIMARY KEY REFERENCES collection(id) ON DELETE CASCADE
);
CREATE TABLE smart_rule_node (
  collection_id INTEGER NOT NULL REFERENCES smart_collection(collection_id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL CHECK(typeof(ordinal)='integer' AND ordinal BETWEEN 0 AND 127),
  parent INTEGER CHECK(parent IS NULL OR (typeof(parent)='integer' AND parent>=0 AND parent<ordinal)),
  kind INTEGER NOT NULL CHECK(typeof(kind)='integer' AND kind BETWEEN 0 AND 2),
  field TEXT,
  text_value TEXT,
  int_value INTEGER,
  PRIMARY KEY(collection_id,ordinal),
  CHECK((ordinal=0 AND parent IS NULL AND kind IN(0,1)) OR (ordinal>0 AND parent IS NOT NULL)),
  CHECK((kind IN(0,1) AND field IS NULL AND text_value IS NULL AND int_value IS NULL) OR
        (kind=2 AND field IS NOT NULL AND (
          (field IN('format','captured-from','captured-to','camera','lens','keyword','text')
             AND typeof(text_value)='text' AND length(CAST(text_value AS BLOB)) BETWEEN 1 AND 1024 AND int_value IS NULL) OR
          (field IN('min-rating','max-rating','label') AND text_value IS NULL AND typeof(int_value)='integer'
             AND int_value BETWEEN 0 AND CASE WHEN field='label' THEN 7 ELSE 5 END)
        )))
);
INSERT INTO smart_collection SELECT collection_id FROM smart_collection_v8;
INSERT INTO smart_rule_node(collection_id,ordinal,parent,kind) SELECT collection_id,0,NULL,0 FROM smart_collection_v8;
INSERT INTO smart_rule_node(collection_id,ordinal,parent,kind,field,text_value) SELECT collection_id,1,0,2,'format',format FROM smart_collection_v8 WHERE format IS NOT NULL;
INSERT INTO smart_rule_node(collection_id,ordinal,parent,kind,field,text_value) SELECT collection_id,2,0,2,'captured-from',captured_from FROM smart_collection_v8 WHERE captured_from IS NOT NULL;
INSERT INTO smart_rule_node(collection_id,ordinal,parent,kind,field,text_value) SELECT collection_id,3,0,2,'captured-to',captured_to FROM smart_collection_v8 WHERE captured_to IS NOT NULL;
INSERT INTO smart_rule_node(collection_id,ordinal,parent,kind,field,text_value) SELECT collection_id,4,0,2,'camera',camera FROM smart_collection_v8 WHERE camera IS NOT NULL;
INSERT INTO smart_rule_node(collection_id,ordinal,parent,kind,field,text_value) SELECT collection_id,5,0,2,'lens',lens FROM smart_collection_v8 WHERE lens IS NOT NULL;
INSERT INTO smart_rule_node(collection_id,ordinal,parent,kind,field,int_value) SELECT collection_id,6,0,2,'min-rating',min_rating FROM smart_collection_v8 WHERE min_rating IS NOT NULL;
INSERT INTO smart_rule_node(collection_id,ordinal,parent,kind,field,int_value) SELECT collection_id,7,0,2,'max-rating',max_rating FROM smart_collection_v8 WHERE max_rating IS NOT NULL;
INSERT INTO smart_rule_node(collection_id,ordinal,parent,kind,field,int_value) SELECT collection_id,8,0,2,'label',label FROM smart_collection_v8 WHERE label IS NOT NULL;
INSERT INTO smart_rule_node(collection_id,ordinal,parent,kind,field,text_value) SELECT collection_id,9,0,2,'keyword',keyword FROM smart_collection_v8 WHERE keyword IS NOT NULL;
INSERT INTO smart_rule_node(collection_id,ordinal,parent,kind,field,text_value) SELECT collection_id,10,0,2,'text',text FROM smart_collection_v8 WHERE text IS NOT NULL;
DROP TABLE smart_collection_v8;
CREATE TRIGGER smart_no_manual_members BEFORE INSERT ON collection_asset
WHEN EXISTS (SELECT 1 FROM smart_collection WHERE collection_id = NEW.collection_id)
BEGIN SELECT RAISE(ABORT, 'Smart Collection membership is determined by its rules'); END;
CREATE TRIGGER smart_no_manual_member_updates BEFORE UPDATE ON collection_asset
WHEN EXISTS (SELECT 1 FROM smart_collection WHERE collection_id = NEW.collection_id)
BEGIN SELECT RAISE(ABORT, 'Smart Collection membership is determined by its rules'); END;
CREATE TRIGGER smart_no_existing_members BEFORE INSERT ON smart_collection
WHEN EXISTS (SELECT 1 FROM collection_asset WHERE collection_id = NEW.collection_id)
BEGIN SELECT RAISE(ABORT, 'Cannot apply smart rules to a manual collection'); END;
CREATE TRIGGER smart_collection_revision_insert AFTER INSERT ON smart_collection BEGIN
  UPDATE sync_revision SET revision=revision+1 WHERE kind='collection' AND entity_id=NEW.collection_id;
END;
CREATE TRIGGER smart_collection_revision_update AFTER UPDATE ON smart_collection BEGIN
  UPDATE sync_revision SET revision=revision+1 WHERE kind='collection' AND entity_id=NEW.collection_id;
  UPDATE sync_revision SET revision=revision+1 WHERE kind='collection' AND entity_id=OLD.collection_id AND OLD.collection_id!=NEW.collection_id;
END;
CREATE TRIGGER smart_collection_revision_delete AFTER DELETE ON smart_collection BEGIN
  UPDATE sync_revision SET revision=revision+1 WHERE kind='collection' AND entity_id=OLD.collection_id;
END;
CREATE TRIGGER smart_collection_generation_insert AFTER INSERT ON smart_collection BEGIN
  UPDATE catalogue SET generation=generation+1;
END;
CREATE TRIGGER smart_collection_generation_update AFTER UPDATE ON smart_collection BEGIN
  UPDATE catalogue SET generation=generation+1;
END;
CREATE TRIGGER smart_collection_generation_delete AFTER DELETE ON smart_collection BEGIN
  UPDATE catalogue SET generation=generation+1;
END;
CREATE TRIGGER smart_rule_node_insert AFTER INSERT ON smart_rule_node BEGIN
  UPDATE sync_revision SET revision=revision+1 WHERE kind='collection' AND entity_id=NEW.collection_id;
  UPDATE catalogue SET generation=generation+1;
END;
CREATE TRIGGER smart_rule_node_update AFTER UPDATE ON smart_rule_node BEGIN
  UPDATE sync_revision SET revision=revision+1 WHERE kind='collection' AND entity_id=NEW.collection_id;
  UPDATE catalogue SET generation=generation+1;
  UPDATE sync_revision SET revision=revision+1 WHERE kind='collection' AND entity_id=OLD.collection_id AND OLD.collection_id!=NEW.collection_id;
END;
CREATE TRIGGER smart_rule_node_delete AFTER DELETE ON smart_rule_node BEGIN
  UPDATE sync_revision SET revision=revision+1 WHERE kind='collection' AND entity_id=OLD.collection_id;
  UPDATE catalogue SET generation=generation+1;
END;
UPDATE catalogue SET schema_version=9;
