//! Saving by extension writes the right OOXML package flavour (.docx/.docm/.dotx/.dotm), and a
//! macro-enabled document keeps its VBA project (as opaque bytes) when it is saved again.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use wordcraft_doc::Document;
use wordcraft_engine::io::{open_bytes, save_bytes};

const CT_DOCX: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
const CT_DOCM: &str = "application/vnd.ms-word.document.macroEnabled.main+xml";
const CT_DOTX: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml";
const CT_DOTM: &str = "application/vnd.ms-word.template.macroEnabledTemplate.main+xml";
const CT_VBA: &str = "application/vnd.ms-office.vbaProject";
const CT_VBA_DATA: &str = "application/vnd.ms-word.vbaData+xml";
const REL_VBA: &str = "http://schemas.microsoft.com/office/2006/relationships/vbaProject";
const REL_VBA_DATA: &str = "http://schemas.microsoft.com/office/2006/relationships/wordVbaData";
const VBA_DATA: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><wne:vbaSuppData xmlns:wne="http://schemas.microsoft.com/office/word/2006/wordml"><wne:mcds><wne:mcd wne:macroName="PROJECT.NEWMACROS.HELLO" wne:name="Project.NewMacros.Hello" wne:bEncrypt="00" wne:cmg="56"/></wne:mcds></wne:vbaSuppData>"#;

fn unzip(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(Cursor::new(bytes)).expect("zip");
    let mut out = BTreeMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).expect("entry");
        let mut b = Vec::new();
        f.read_to_end(&mut b).expect("read entry");
        out.insert(f.name().to_string(), b);
    }
    out
}

fn zip(entries: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut zw = zip::ZipWriter::new(Cursor::new(Vec::new()));
    // Content types first, as Word writes them.
    let order = entries.iter().filter(|(n, _)| *n == "[Content_Types].xml").chain(entries.iter().filter(|(n, _)| *n != "[Content_Types].xml"));
    for (name, bytes) in order {
        zw.start_file(name.as_str(), zip::write::SimpleFileOptions::default()).expect("start");
        zw.write_all(bytes).expect("write");
    }
    zw.finish().expect("finish").into_inner()
}

fn text(files: &BTreeMap<String, Vec<u8>>, name: &str) -> String {
    String::from_utf8_lossy(files.get(name).map(Vec::as_slice).unwrap_or_default()).into_owned()
}

/// A recognisable stand-in for a compiled VBA project (an OLE compound file in real life).
fn fake_vba() -> Vec<u8> {
    let mut v = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
    v.extend((0..4000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 11) as u8));
    v
}

/// A `.docm` the way Word lays one out: macro-enabled main part, `word/vbaProject.bin` related
/// from the document, and `word/vbaData.xml` related from the VBA project.
fn docm_package(vba: &[u8], main_ct: &str) -> Vec<u8> {
    let doc = Document::from_text("Macro host\nSecond paragraph");
    let mut files = unzip(&wordcraft_docx::write(&doc).expect("write"));
    let ct = text(&files, "[Content_Types].xml")
        .replace(CT_DOCX, main_ct)
        .replace(r#"<Default Extension="xml""#, &format!(r#"<Default Extension="bin" ContentType="{CT_VBA}"/><Default Extension="xml""#))
        .replace("</Types>", &format!(r#"<Override PartName="/word/vbaData.xml" ContentType="{CT_VBA_DATA}"/></Types>"#));
    files.insert("[Content_Types].xml".into(), ct.into_bytes());
    let rels = text(&files, "word/_rels/document.xml.rels")
        .replace("</Relationships>", &format!(r#"<Relationship Id="rId99" Type="{REL_VBA}" Target="vbaProject.bin"/></Relationships>"#));
    files.insert("word/_rels/document.xml.rels".into(), rels.into_bytes());
    files.insert("word/vbaProject.bin".into(), vba.to_vec());
    let vba_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL_VBA_DATA}" Target="vbaData.xml"/></Relationships>"#
    );
    files.insert("word/_rels/vbaProject.bin.rels".into(), vba_rels.into_bytes());
    files.insert("word/vbaData.xml".into(), VBA_DATA.as_bytes().to_vec());
    zip(&files)
}

/// The content type the package declares for `part` (Override first, then Default by extension).
fn content_type(files: &BTreeMap<String, Vec<u8>>, part: &str) -> Option<String> {
    let ct = text(files, "[Content_Types].xml");
    let attr = |tag: &str, name: &str| tag.split(&format!("{name}=\"")).nth(1).and_then(|s| s.split('"').next()).map(str::to_string);
    let tags: Vec<&str> = ct.split('<').collect();
    if let Some(t) = tags.iter().find(|t| t.starts_with("Override") && attr(t, "PartName").as_deref() == Some(part)) {
        return attr(t, "ContentType");
    }
    let ext = part.rsplit('.').next()?;
    tags.iter().find(|t| t.starts_with("Default") && attr(t, "Extension").as_deref() == Some(ext)).and_then(|t| attr(t, "ContentType"))
}

/// Every internal relationship target in the package exists, and every part has a content type.
fn assert_package_consistent(files: &BTreeMap<String, Vec<u8>>) {
    for (name, bytes) in files.iter().filter(|(n, _)| n.ends_with(".rels")) {
        let src_dir = name.trim_end_matches(".rels").rsplit_once("_rels/").map(|(d, _)| d.trim_end_matches('/')).unwrap_or("");
        let xml = String::from_utf8_lossy(bytes);
        for rel in xml.split("<Relationship ").skip(1) {
            if rel.contains("TargetMode=\"External\"") {
                continue;
            }
            let target = rel.split("Target=\"").nth(1).and_then(|s| s.split('"').next()).unwrap_or("");
            let path = if src_dir.is_empty() { target.to_string() } else { format!("{src_dir}/{target}") };
            assert!(files.contains_key(&path), "{name} points at missing part {path}");
        }
    }
    for name in files.keys().filter(|n| *n != "[Content_Types].xml" && !n.ends_with(".rels")) {
        assert!(content_type(files, &format!("/{name}")).is_some(), "{name} has no content type");
    }
}

#[test]
fn docm_keeps_its_macros_when_saved_again() {
    let vba = fake_vba();
    let doc = open_bytes("m.docm", &docm_package(&vba, CT_DOCM)).expect("open docm");
    let out = unzip(&save_bytes("m.docm", &doc).expect("save docm"));

    assert_eq!(content_type(&out, "/word/document.xml").as_deref(), Some(CT_DOCM));
    assert!(out.get("word/vbaProject.bin") == Some(&vba), "vbaProject.bin must survive byte for byte");
    assert_eq!(content_type(&out, "/word/vbaProject.bin").as_deref(), Some(CT_VBA));
    let rels = text(&out, "word/_rels/document.xml.rels");
    assert!(rels.contains(REL_VBA) && rels.contains("Target=\"vbaProject.bin\""), "{rels}");
    let vba_rels = text(&out, "word/_rels/vbaProject.bin.rels");
    assert!(vba_rels.contains(REL_VBA_DATA) && vba_rels.contains("Target=\"vbaData.xml\""), "{vba_rels}");
    assert_eq!(text(&out, "word/vbaData.xml"), VBA_DATA);
    assert_eq!(content_type(&out, "/word/vbaData.xml").as_deref(), Some(CT_VBA_DATA));
    assert_package_consistent(&out);

    // And again: a second generation is identical in the parts that matter.
    let again = unzip(&save_bytes("m.docm", &open_bytes("m.docm", &zip(&out)).expect("reopen")).expect("resave"));
    assert!(again.get("word/vbaProject.bin") == Some(&vba), "vbaProject.bin must survive byte for byte");
    assert_eq!(text(&again, "word/vbaData.xml"), VBA_DATA);
}

#[test]
fn dotm_keeps_its_macros_when_saved_again() {
    let vba = fake_vba();
    let doc = open_bytes("t.dotm", &docm_package(&vba, CT_DOTM)).expect("open dotm");
    let out = unzip(&save_bytes("t.dotm", &doc).expect("save dotm"));
    assert_eq!(content_type(&out, "/word/document.xml").as_deref(), Some(CT_DOTM));
    assert!(out.get("word/vbaProject.bin") == Some(&vba), "vbaProject.bin must survive byte for byte");
    assert_eq!(text(&out, "word/vbaData.xml"), VBA_DATA);
    assert_package_consistent(&out);
}

#[test]
fn each_extension_gets_its_main_content_type() {
    let doc = Document::from_text("Plain");
    for (name, ct) in [("a.docx", CT_DOCX), ("a.docm", CT_DOCM), ("a.dotx", CT_DOTX), ("a.dotm", CT_DOTM), ("A.DOCM", CT_DOCM)] {
        let out = unzip(&save_bytes(name, &doc).unwrap_or_else(|e| panic!("save {name}: {e}")));
        assert_eq!(content_type(&out, "/word/document.xml").as_deref(), Some(ct), "{name}");
        // No macros to carry: a valid macro-enabled package simply has no VBA parts.
        assert!(!out.keys().any(|k| k.contains("vba")), "{name}: {:?}", out.keys());
        assert!(!text(&out, "[Content_Types].xml").contains("vbaProject"), "{name}");
        assert_package_consistent(&out);
    }
}

#[test]
fn saving_macros_as_docx_drops_them_cleanly() {
    let vba = fake_vba();
    let doc = open_bytes("m.docm", &docm_package(&vba, CT_DOCM)).expect("open docm");
    for name in ["m.docx", "m.dotx"] {
        let out = unzip(&save_bytes(name, &doc).expect("save"));
        assert!(!out.keys().any(|k| k.contains("vba")), "{name}: {:?}", out.keys());
        assert!(!text(&out, "[Content_Types].xml").contains("vba"), "{name}: no VBA content type left behind");
        assert!(!text(&out, "word/_rels/document.xml.rels").contains("vbaProject"), "{name}: no dangling relationship");
        assert_package_consistent(&out);
    }
    // The open document still holds the project: Save As .docm afterwards keeps it.
    let back = unzip(&save_bytes("m.docm", &doc).expect("save docm"));
    assert!(back.get("word/vbaProject.bin") == Some(&vba), "vbaProject.bin must survive byte for byte");
}

#[test]
fn hostile_vba_parts_never_panic() {
    let vba = fake_vba();
    let pkg = docm_package(&vba, CT_DOCM);
    let base = unzip(&pkg);
    let mut cases: Vec<(&str, BTreeMap<String, Vec<u8>>)> = Vec::new();

    // Relationship to a part that isn't there.
    let mut missing = base.clone();
    missing.remove("word/vbaProject.bin");
    cases.push(("missing vbaProject.bin", missing));
    // VBA data relationship to a missing part, and malformed VBA data XML.
    let mut no_data = base.clone();
    no_data.remove("word/vbaData.xml");
    cases.push(("missing vbaData.xml", no_data));
    let mut bad_data = base.clone();
    bad_data.insert("word/vbaData.xml".into(), b"<wne:vbaSuppData><unclosed".to_vec());
    cases.push(("malformed vbaData.xml", bad_data));
    // Empty and junk project bytes.
    let mut empty = base.clone();
    empty.insert("word/vbaProject.bin".into(), Vec::new());
    cases.push(("empty vbaProject.bin", empty));
    // Relationship pointing outside the package / at the main part / external.
    for (label, target) in
        [("escaping target", "../../../../etc/passwd"), ("self target", "document.xml"), ("rels target", "_rels/document.xml.rels")]
    {
        let mut odd = base.clone();
        let rels = text(&odd, "word/_rels/document.xml.rels").replace("Target=\"vbaProject.bin\"", &format!("Target=\"{target}\""));
        odd.insert("word/_rels/document.xml.rels".into(), rels.into_bytes());
        cases.push((label, odd));
    }
    let mut external = base.clone();
    let rels = text(&external, "word/_rels/document.xml.rels")
        .replace("Target=\"vbaProject.bin\"", "Target=\"https://example.com/x.bin\" TargetMode=\"External\"");
    external.insert("word/_rels/document.xml.rels".into(), rels.into_bytes());
    cases.push(("external target", external));
    // Two vbaProject relationships, and VBA data pointing back at the project.
    let mut dup = base.clone();
    let rels = text(&dup, "word/_rels/document.xml.rels")
        .replace("</Relationships>", &format!(r#"<Relationship Id="rId98" Type="{REL_VBA}" Target="vbaProject.bin"/></Relationships>"#));
    dup.insert("word/_rels/document.xml.rels".into(), rels.into_bytes());
    let vrels = text(&dup, "word/_rels/vbaProject.bin.rels").replace("vbaData.xml", "vbaProject.bin");
    dup.insert("word/_rels/vbaProject.bin.rels".into(), vrels.into_bytes());
    cases.push(("duplicate and cyclic", dup));

    for (label, files) in cases {
        // A broken macro part never stops the document itself from opening.
        let doc = open_bytes("h.docm", &zip(&files)).unwrap_or_else(|e| panic!("{label}: open: {e}"));
        for name in ["h.docm", "h.dotm", "h.docx"] {
            let out = unzip(&save_bytes(name, &doc).unwrap_or_else(|e| panic!("{label}: save {name}: {e}")));
            assert_package_consistent(&out);
            assert!(wordcraft_docx::read(&zip(&out)).is_ok(), "{label}: {name} reads back");
            if let Some(bin) = out.get("word/vbaProject.bin") {
                assert!(name != "h.docx", "{label}: no VBA in a .docx");
                assert!(!bin.is_empty(), "{label}: never write an empty VBA project");
            }
        }
    }
}
