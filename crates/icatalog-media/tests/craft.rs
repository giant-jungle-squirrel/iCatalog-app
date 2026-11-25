//! Synthetic files only: no camera files, artwork or personal metadata.
use icatalog_media::{prepare_media, prepare_media_with_orientation};
use std::io::Cursor;

fn png() -> Vec<u8> {
    let image = image::RgbImage::from_pixel(8, 4, image::Rgb([200, 60, 20]));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    bytes.into_inner()
}
fn psd(version: u16, resource: &[u8]) -> Vec<u8> {
    let mut b = b"8BPS".to_vec();
    b.extend(version.to_be_bytes());
    b.extend([0; 6]);
    b.extend(3u16.to_be_bytes());
    b.extend(4u32.to_be_bytes());
    b.extend(8u32.to_be_bytes());
    b.extend(8u16.to_be_bytes());
    b.extend(3u16.to_be_bytes());
    b.extend(0u32.to_be_bytes());
    b.extend((resource.len() as u32).to_be_bytes());
    b.extend(resource);
    b.extend(vec![0; if version == 2 { 8 } else { 4 }]);
    b.extend(0u16.to_be_bytes());
    for value in [200, 60, 20] {
        b.extend(vec![value; 32]);
    }
    b
}
fn resource(id: u16, data: &[u8]) -> Vec<u8> {
    let mut b = b"8BIM".to_vec();
    b.extend(id.to_be_bytes());
    b.extend([0, 0]);
    b.extend((data.len() as u32).to_be_bytes());
    b.extend(data);
    if !data.len().is_multiple_of(2) {
        b.push(0);
    }
    b
}
fn dng(width: u32, height: u32) -> Vec<u8> {
    let mut tags = vec![
        (256, 4, 1, width),
        (257, 4, 1, height),
        (258, 3, 1, 16),
        (259, 3, 1, 1),
        (262, 3, 1, 32803),
        (273, 4, 1, 0),
        (274, 3, 1, 6),
        (277, 3, 1, 1),
        (278, 4, 1, height),
        (279, 4, 1, 64),
        (33421, 3, 2, 0x00020002),
        (33422, 1, 4, 0x02010100),
        (50706, 1, 4, 0x00000401),
        (50717, 4, 1, 65535),
    ];
    let offset = 8 + 2 + tags.len() as u32 * 12 + 4;
    tags.iter_mut().find(|t| t.0 == 273).unwrap().3 = offset;
    let mut b = b"II*\0\x08\0\0\0".to_vec();
    b.extend((tags.len() as u16).to_le_bytes());
    for (tag, kind, count, value) in tags {
        b.extend((tag as u16).to_le_bytes());
        b.extend((kind as u16).to_le_bytes());
        b.extend((count as u32).to_le_bytes());
        b.extend(value.to_le_bytes());
    }
    b.extend(0u32.to_le_bytes());
    for _ in 0..32 {
        b.extend(16384u16.to_le_bytes());
    }
    b
}

#[test]
fn raster_orientation_override_is_shared_and_source_size_survives() {
    let p = prepare_media_with_orientation(&png(), "png", 256, Some(6)).unwrap();
    assert_eq!((p.width, p.height), (Some(8), Some(4)));
    assert_eq!((p.thumbnail.width, p.thumbnail.height), (4, 8));
    assert!(p.thumbnail.orientation_applied);
    assert_eq!(p.format, "png");
}
#[test]
fn psd_and_psb_merged_composites_skip_layers_and_preserve_canvas() {
    for version in [1, 2] {
        let p = prepare_media(&psd(version, &[]), "psd", 256).unwrap();
        assert_eq!((p.width, p.height), (Some(8), Some(4)));
        assert_eq!(p.format, if version == 1 { "psd" } else { "psb" });
        assert_eq!(p.source, "rendered");
        assert_eq!((p.thumbnail.width, p.thumbnail.height), (8, 4));
    }
}
#[test]
fn psd_saved_thumbnail_and_xmp_precede_unusable_composite() {
    let jpeg = icatalog_media::thumbnail_jpeg(&png(), 256).unwrap().bytes;
    let thumbnail = craft_psd::resources::Thumbnail::jfif_bytes(8, 4, &jpeg).unwrap();
    let xml="<x:xmpmeta xmlns:x='adobe:ns:meta/'><rdf:RDF xmlns:rdf='http://www.w3.org/1999/02/22-rdf-syntax-ns#'/></x:xmpmeta>";
    let mut resources = resource(1036, &thumbnail);
    resources.extend(resource(1060, xml.as_bytes()));
    let mut bytes = psd(1, &resources);
    bytes.truncate(bytes.len() - 98);
    let p = prepare_media(&bytes, "psd", 256).unwrap();
    assert_eq!(p.source, "psd_resource");
    assert_eq!(p.xmp_packet.as_deref(), Some(xml));
}
#[test]
fn previewless_dng_is_demosaiced_and_oriented() {
    let p = prepare_media(&dng(8, 4), "dng", 256).unwrap();
    assert_eq!(p.source, "rendered");
    assert_eq!(p.format, "dng");
    assert_eq!((p.width, p.height), (Some(8), Some(4)));
    assert_eq!((p.thumbnail.width, p.thumbnail.height), (4, 8));
    let pixels = icatalog_media::decode_rgba(&p.thumbnail.bytes).unwrap();
    assert!(pixels
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| p[0] > 80 && p[0] < 240));
}
#[test]
fn oversized_raw_and_psd_are_refused_before_decode() {
    assert!(prepare_media(&dng(50000, 50000), "dng", 256).is_err());
    let mut bytes = psd(1, &[]);
    bytes[14..18].copy_from_slice(&50000u32.to_be_bytes());
    bytes[18..22].copy_from_slice(&50000u32.to_be_bytes());
    assert!(prepare_media(&bytes, "psd", 256).is_err());
    assert!(prepare_media(&png(), "png", u32::MAX).is_err());
}
#[test]
fn openraster_composite_falls_back_to_thumbnail_without_inventing_canvas_size() {
    let mut writer = craft_ora::Writer::new();
    writer
        .add("stack.xml", b"<image w='800' h='400'><stack/></image>")
        .unwrap();
    writer.add("mergedimage.png", b"damaged").unwrap();
    writer.add("Thumbnails/thumbnail.png", &png()).unwrap();
    let p = prepare_media(&writer.finish().unwrap(), "ora", 256).unwrap();
    assert_eq!((p.width, p.height), (Some(800), Some(400)));
    assert_eq!((p.thumbnail.width, p.thumbnail.height), (8, 4));
}
#[test]
fn eps_and_legacy_illustrator_use_epsi_without_executing_postscript() {
    let bytes=b"%!PS-Adobe-3.0 EPSF-3.0\n%%BoundingBox: 0 0 100 100\n%%BeginPreview: 2 1 8 1\n% 00ff\n%%EndPreview\nmalicious unknown operators\n";
    for hint in ["eps", "ai"] {
        let p = prepare_media(bytes, hint, 256).unwrap();
        assert_eq!((p.width, p.height), (None, None));
        assert_eq!((p.thumbnail.width, p.thumbnail.height), (2, 1));
    }
    assert!(prepare_media(b"%!PS-Adobe-3.0 EPSF-3.0\nshowpage", "eps", 256).is_err());
}
#[test]
fn hostile_and_truncated_containers_do_not_escape() {
    for hint in ["psd", "psb", "dng", "cr3", "kra", "ora", "afphoto", "eps"] {
        for bytes in [b"".as_slice(), b"junk", &[255; 256], &[0; 256]] {
            assert!(prepare_media(bytes, hint, 256).is_err(), "{hint}");
        }
    }
    for bytes in [psd(1, &[]), dng(8, 4)] {
        for end in 0..bytes.len() {
            let _ = prepare_media(
                &bytes[..end],
                if bytes.starts_with(b"8BPS") {
                    "psd"
                } else {
                    "dng"
                },
                64,
            );
        }
    }
}

#[test]
fn krita_uses_saved_composite_and_rejects_missing_saved_pixels() {
    let archive = |pixels: bool| {
        let mut writer = craft_zip::Writer::new();
        for (name, data) in [
            ("mimetype", b"application/x-krita".as_slice()),
            (
                "maindoc.xml",
                b"<DOC><IMAGE width='800' height='400' colorspacename='RGBA'/></DOC>".as_slice(),
            ),
        ] {
            writer
                .add(name, data, craft_zip::Compression::Stored)
                .unwrap();
        }
        if pixels {
            writer
                .add("mergedimage.png", &png(), craft_zip::Compression::Stored)
                .unwrap();
        }
        writer.finish().unwrap()
    };
    let p = prepare_media(&archive(true), "kra", 256).unwrap();
    assert_eq!((p.width, p.height), (Some(800), Some(400)));
    assert_eq!(p.format, "kra");
    assert!(prepare_media(&archive(false), "kra", 256).is_err());
}
#[test]
fn affinity_checks_indexed_thumbnail_and_does_not_report_thumbnail_as_canvas() {
    let png = png();
    let mut bytes = b"\0\xffKA".to_vec();
    bytes.extend(12u16.to_le_bytes());
    bytes.extend(0u16.to_le_bytes());
    bytes.extend(b"nsrP#Inf");
    bytes.extend(0u64.to_le_bytes());
    bytes.extend(72u64.to_le_bytes());
    bytes.extend([0; 32]);
    bytes.extend(b"Prot");
    bytes.extend([0; 4]);
    bytes.extend(b"\xff\xff\xff\xffThmb");
    for v in [1, png.len() as u32 + 13, 29, 0, png.len() as u32] {
        bytes.extend(v.to_le_bytes());
    }
    bytes.push(1);
    bytes.extend(png);
    let p = prepare_media(&bytes, "afphoto", 256).unwrap();
    assert_eq!((p.width, p.height), (None, None));
    assert_eq!((p.thumbnail.width, p.thumbnail.height), (8, 4));
    *bytes.last_mut().unwrap() ^= 1;
    assert!(prepare_media(&bytes, "afphoto", 256).is_err());
}
#[test]
fn raw_embedded_preview_wins_over_sensor_conversion() {
    let jpeg = icatalog_media::thumbnail_jpeg(&png(), 256).unwrap().bytes;
    let mut bytes = dng(8, 4);
    let preview_offset = bytes.len() as u32 + 24;
    let mut entries = Vec::new();
    for (tag, value) in [(513u16, preview_offset), (514, jpeg.len() as u32)] {
        entries.extend(tag.to_le_bytes());
        entries.extend(4u16.to_le_bytes());
        entries.extend(1u32.to_le_bytes());
        entries.extend(value.to_le_bytes());
    }
    bytes[8..10].copy_from_slice(&16u16.to_le_bytes());
    bytes[78..82].copy_from_slice(&206u32.to_le_bytes());
    bytes.splice(178..178, entries);
    bytes.extend(jpeg);
    let p = prepare_media(&bytes, "dng", 256).unwrap();
    assert_eq!(p.source, "raw_preview");
    assert_eq!((p.width, p.height), (Some(8), Some(4)));
}

#[test]
fn cr3_preserves_container_xmp_and_cmt_orientation_with_saved_preview() {
    let bx = |kind: &[u8; 4], data: &[u8]| {
        let mut bytes = ((data.len() + 8) as u32).to_be_bytes().to_vec();
        bytes.extend(kind);
        bytes.extend(data);
        bytes
    };
    let cmt = craft_tiff::TiffWriter::new(craft_tiff::ByteOrder::Little, false)
        .write(&[craft_tiff::IfdBuilder::new()
            .with(274, craft_tiff::Value::Short(vec![6]))
            .with(271, craft_tiff::Value::Ascii("Synthetic".into()))])
        .unwrap();
    let mut canon = vec![
        0x85, 0xc0, 0xb6, 0x87, 0x82, 0x0f, 0x11, 0xe0, 0x81, 0x11, 0xf4, 0xce, 0x46, 0x2b, 0x6a,
        0x48,
    ];
    canon.extend(bx(b"CMT1", &cmt));
    let packet = "<x:xmpmeta xmlns:x='adobe:ns:meta/'/>";
    let mut xmp = craft_metadata::cr3::XMP_UUID.to_vec();
    xmp.extend(packet.as_bytes());
    let mut bytes = bx(b"ftyp", b"crx \0\0\0\0crx ");
    bytes.extend(bx(b"moov", &bx(b"uuid", &canon)));
    bytes.extend(bx(b"uuid", &xmp));
    bytes.extend(bx(
        b"mdat",
        &icatalog_media::thumbnail_jpeg(&png(), 256).unwrap().bytes,
    ));
    let p = prepare_media(&bytes, "cr3", 256).unwrap();
    assert_eq!(p.source, "raw_preview");
    assert_eq!(p.xmp_packet.as_deref(), Some(packet));
    assert_eq!(p.exif.make.as_deref(), Some("Synthetic"));
    assert_eq!((p.thumbnail.width, p.thumbnail.height), (4, 8));
}

#[test]
fn psd_and_psb_rle_zip_prediction_composites_are_bounded() {
    for version in [1, 2] {
        for compression in [
            craft_psd::Compression::Rle,
            craft_psd::Compression::Zip,
            craft_psd::Compression::ZipPrediction,
        ] {
            let mut bytes = psd(version, &[]);
            let header = craft_psd::HeadView::from_bytes(&bytes).unwrap().header;
            let planes: Vec<u8> = [200, 60, 20]
                .into_iter()
                .flat_map(|v| vec![v; 32])
                .collect();
            let encoded = craft_psd::ImageData::encode(compression, &planes, &header).unwrap();
            bytes.truncate(if version == 1 { 38 } else { 42 });
            bytes.extend(compression.as_u16().to_be_bytes());
            bytes.extend(encoded.encoded().unwrap().as_ref());
            let p = prepare_media(&bytes, "psd", 256).unwrap();
            assert_eq!((p.thumbnail.width, p.thumbnail.height), (8, 4));
            let pixels = icatalog_media::decode_rgba(&p.thumbnail.bytes).unwrap();
            assert!(pixels
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0] > 180 && p[1] < 80 && p[2] < 40));
            bytes[14..18].copy_from_slice(&5000u32.to_be_bytes());
            bytes[18..22].copy_from_slice(&4000u32.to_be_bytes());
            bytes[22..24].copy_from_slice(&32u16.to_be_bytes());
            assert!(prepare_media(&bytes, "psd", 256).is_err());
        }
    }
}

#[test]
fn psd_placeholder_is_not_imported_but_usable_thumbnail_survives() {
    let flag = craft_psd::resources::version_info_resource(false);
    let mut resources = resource(flag.id, &flag.data);
    assert!(prepare_media(&psd(1, &resources), "psd", 256).is_err());
    let jpeg = icatalog_media::thumbnail_jpeg(&png(), 256).unwrap().bytes;
    resources.extend(resource(
        1036,
        &craft_psd::resources::Thumbnail::jfif_bytes(8, 4, &jpeg).unwrap(),
    ));
    assert_eq!(
        prepare_media(&psd(1, &resources), "psd", 256)
            .unwrap()
            .source,
        "psd_resource"
    );
}
#[test]
fn saved_document_alpha_is_composited_on_white() {
    for (alpha, expected) in [(0, 255), (128, 127)] {
        let image = image::RgbaImage::from_pixel(8, 4, image::Rgba([255, 0, 0, alpha]));
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, image::ImageFormat::Png).unwrap();
        let mut writer = craft_ora::Writer::new();
        writer
            .add("stack.xml", b"<image w='8' h='4'><stack/></image>")
            .unwrap();
        writer.add("mergedimage.png", png.get_ref()).unwrap();
        let p = prepare_media(&writer.finish().unwrap(), "ora", 256).unwrap();
        let pixels = icatalog_media::decode_rgba(&p.thumbnail.bytes).unwrap();
        assert!(pixels.rgba.as_chunks::<4>().0.iter().all(|p| p[0] > 245
            && (i16::from(p[1]) - expected).abs() < 5
            && (i16::from(p[2]) - expected).abs() < 5));
    }
}

#[test]
fn photoshop_four_thumbnail_samples_are_bgr() {
    let jpeg = icatalog_media::thumbnail_jpeg(&png(), 256).unwrap().bytes;
    let thumbnail = craft_psd::resources::Thumbnail::jfif_bytes(8, 4, &jpeg).unwrap();
    let p = prepare_media(&psd(1, &resource(1033, &thumbnail)), "psd", 256).unwrap();
    let pixels = icatalog_media::decode_rgba(&p.thumbnail.bytes).unwrap();
    assert!(pixels
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| p[0] < 40 && p[1] < 80 && p[2] > 180));
}
