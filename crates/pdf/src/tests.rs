use std::sync::Arc;

use wordcraft_doc::para::InlineObject;
use wordcraft_doc::props::NumRef;
use wordcraft_doc::{Block, CharProps, Document, ListKind, Paragraph, Table, Watermark, para_block};

use crate::*;

fn png() -> Vec<u8> {
    let img = image::RgbaImage::from_fn(16, 8, |x, y| image::Rgba([(x * 15) as u8, (y * 30) as u8, 200, 255]));
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png).unwrap();
    buf
}

/// Headings, a bold run, a hyperlink, an internal link, a list, a table and a picture.
fn sample() -> Document {
    let mut d = Document::new();
    d.core.title = "PDF Sample".into();
    d.core.creator = "Ada Writer".into();
    d.core.subject = "Testing".into();
    d.core.keywords = "pdf, export".into();
    d.core.created = "2024-05-06T07:08:09Z".into();
    let mut b = Vec::new();
    let mut h1 = Paragraph::with_text("Introduction", CharProps::default()).styled("Heading1");
    let n = h1.len();
    h1.insert_object(n, InlineObject::BookmarkStart { name: "intro".into() }, &CharProps::default()).unwrap();
    b.push(para_block(h1));
    let mut p = Paragraph::with_text("Hello WordCraft, ", CharProps::default());
    let n = p.len();
    p.insert_text(n, "bold words", &CharProps { bold: Some(true), ..Default::default() }).unwrap();
    let n = p.len();
    p.insert_text(n, " and a website", &CharProps { link: Some("https://example.com/".into()), ..Default::default() }).unwrap();
    let n = p.len();
    p.insert_text(n, " back to top", &CharProps { link: Some("#intro".into()), italic: Some(true), ..Default::default() }).unwrap();
    b.push(para_block(p));
    b.push(para_block(Paragraph::with_text("Details", CharProps::default()).styled("Heading2")));
    let list = d.numbering.add_list(ListKind::Numbered);
    for t in ["alpha item", "beta item"] {
        let mut q = Paragraph::with_text(t, CharProps::default()).styled("ListParagraph");
        q.props.numbering = Some(NumRef { num: list, level: 0 });
        b.push(para_block(q));
    }
    let mut t = Table::new(2, 2, 400.0);
    for (r, c, s) in [(0, 0, "Cell00"), (0, 1, "Cell01"), (1, 0, "Cell10"), (1, 1, "Cell11")] {
        t.rows[r].cells[c].blocks = vec![para_block(Paragraph::with_text(s, CharProps::default()))];
    }
    b.push(Arc::new(Block::Table(t)));
    let key = d.add_media(png(), "png");
    let mut ip = Paragraph::with_text("Figure: ", CharProps::default());
    let n = ip.len();
    ip.insert_object(
        n,
        InlineObject::Image { media: key, w: 80.0, h: 40.0, alt: "gradient".into(), float: Default::default(), crop: [0.1, 0.0, 0.0, 0.0] },
        &CharProps::default(),
    )
    .unwrap();
    b.push(para_block(ip));
    let mut big = Paragraph::with_text("Second page", CharProps::default()).styled("Heading1");
    big.props.page_break_before = Some(true);
    b.push(para_block(big));
    b.push(para_block(Paragraph::with_text(
        "Dotted and wavy",
        CharProps { underline: Some(wordcraft_doc::props::Underline::Wave), ..Default::default() },
    )));
    d.body = b;
    d.settings.watermark = Some(Watermark::default());
    d.settings.page_color = Some(Rgb(0xFF, 0xFE, 0xF0));
    d
}

/// Text of every page through hayro's interpreter (glyph → Unicode via ToUnicode / ActualText).
fn extract_text(bytes: &[u8]) -> Vec<String> {
    use hayro_interpret::font::Glyph;
    use hayro_interpret::{
        BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image, InterpreterCache, InterpreterSettings, Paint, PathDrawMode, SoftMask,
        interpret_page,
    };
    use hayro_syntax::Pdf;
    struct Ex(String);
    impl Device<'_> for Ex {
        fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
        fn set_blend_mode(&mut self, _: BlendMode) {}
        fn draw_path(&mut self, _: &kurbo::BezPath, _: kurbo::Affine, _: &Paint<'_>, _: &PathDrawMode) {}
        fn push_clip_path(&mut self, _: &ClipPath) {}
        fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
        fn draw_glyph(&mut self, g: &Glyph<'_>, _: kurbo::Affine, _: kurbo::Affine, _: &Paint<'_>, _: &GlyphDrawMode) {
            match g.as_unicode() {
                Some(hayro_cmap::BfString::Char(c)) => self.0.push(c),
                Some(hayro_cmap::BfString::String(s)) => self.0.push_str(&s),
                None => self.0.push('\u{FFFD}'),
            }
        }
        fn draw_image(&mut self, _: Image<'_, '_>, _: kurbo::Affine) {}
        fn pop_clip_path(&mut self) {}
        fn pop_transparency_group(&mut self) {}
    }
    let pdf = Pdf::new(bytes.to_vec()).expect("parse");
    let cache = InterpreterCache::new();
    pdf.pages()
        .iter()
        .map(|page| {
            let mut ctx =
                Context::new(kurbo::Affine::IDENTITY, kurbo::Rect::new(0.0, 0.0, 1.0, 1.0), &cache, pdf.xref(), InterpreterSettings::default());
            let mut ex = Ex(String::new());
            interpret_page(page, &mut ctx, &mut ex);
            ex.0
        })
        .collect()
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn exports_pages_text_and_structure() {
    let d = sample();
    let lay = layout(&d, &mut LayoutCache::new(), &LayoutOptions::default());
    assert!(lay.pages.len() >= 2);
    let bytes = export(&d, &PdfOptions::default()).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    let pdf = hayro_syntax::Pdf::new(bytes.clone()).expect("parse");
    assert_eq!(pdf.pages().len(), lay.pages.len());
    let text = extract_text(&bytes);
    let all = squash(&text.concat());
    for w in [
        "Introduction",
        "Hello WordCraft,",
        "bold words",
        "website",
        "Details",
        "alpha item",
        "beta item",
        "Cell00",
        "Cell11",
        "Second page",
        "DRAFT",
    ] {
        assert!(all.contains(w), "{w:?} missing from {all:?}");
    }
    assert!(squash(&text[0]).contains("Hello WordCraft, bold words and a website back to top"), "{:?}", text[0]);

    let raw = String::from_utf8_lossy(&export(&d, &PdfOptions { compress: false, ..Default::default() }).unwrap()).into_owned();
    assert!(raw.contains("/FontFile2") || raw.contains("/FontFile3"), "embedded fonts");
    assert!(raw.contains("/ToUnicode"));
    assert!(raw.contains("/URI (https://example.com/)") || raw.contains("/URI(https://example.com/)"), "URL link");
    assert!(raw.contains("/Subtype /Link") || raw.contains("/Subtype/Link"));
    assert!(raw.contains("/Outlines"), "outline");
    assert!(raw.contains("Introduction"));
    assert!(raw.contains("/StructTreeRoot"), "tagged");
    assert!(raw.contains("/Image"), "picture");
    assert!(raw.contains("PDF Sample"), "title");
    assert!(raw.contains("Ada Writer"), "author");
    assert!(raw.contains("D:20240506070809"), "creation date");
}

#[test]
fn page_selection_and_errors() {
    let d = sample();
    let one = export(&d, &PdfOptions { pages: Some(vec![1]), tagged: false, ..Default::default() }).unwrap();
    let pdf = hayro_syntax::Pdf::new(one.clone()).unwrap();
    assert_eq!(pdf.pages().len(), 1);
    assert!(squash(&extract_text(&one)[0]).contains("Second page"));
    assert_eq!(export(&d, &PdfOptions { pages: Some(vec![99]), ..Default::default() }), Err(PdfError::BadPage(100)));
    assert_eq!(export(&d, &PdfOptions { pages: Some(vec![]), ..Default::default() }), Err(PdfError::NoPages));
}

#[test]
fn hostile_documents_export() {
    let mut d = Document::from_text("x");
    d.last_section.page_w = f32::NAN;
    d.last_section.page_h = 1e9;
    d.media.insert("bad.png".into(), Arc::new(b"not an image".to_vec()));
    let mut p = Paragraph::with_text("pic ", CharProps::default());
    p.insert_object(
        4,
        InlineObject::Image {
            media: "bad.png".into(),
            w: f32::INFINITY,
            h: -3.0,
            alt: String::new(),
            float: Default::default(),
            crop: [f32::NAN; 4],
        },
        &CharProps::default(),
    )
    .unwrap();
    p.insert_object(
        0,
        InlineObject::Image { media: "missing".into(), w: 10.0, h: 10.0, alt: String::new(), float: Default::default(), crop: [0.0; 4] },
        &CharProps::default(),
    )
    .unwrap();
    d.body.push(para_block(p));
    let mut q =
        Paragraph::with_text("Unknown font 😀 עברית", CharProps { font: Some("No Such Font".into()), size: Some(2000.0), ..Default::default() });
    q.props.style = Some("Heading9".into());
    d.body.push(para_block(q));
    d.settings.watermark = Some(Watermark { text: "   ".into(), ..Default::default() });
    let _ = export(&d, &PdfOptions::default());
    d.settings.watermark = Some(Watermark { text: "😀".repeat(500), font: "Nope".into(), diagonal: false, ..Default::default() });
    let _ = export(&d, &PdfOptions { tagged: false, ..Default::default() });
    assert!(export(&Document::new(), &PdfOptions::default()).unwrap().starts_with(b"%PDF"));
}

#[test]
fn tracked_deletions_are_left_out_without_markup() {
    use wordcraft_doc::{Revision, RevisionKind};
    let mut d = Document::new();
    d.revisions.push(Revision { kind: RevisionKind::Delete, author: "Ada".into(), date: String::new() });
    d.revisions.push(Revision { kind: RevisionKind::Insert, author: "Ada".into(), date: String::new() });
    let mut p = Paragraph::with_text("Keep ", CharProps::default());
    let n = p.len();
    p.insert_text(n, "DELETEDTEXT ", &CharProps { del: Some(0), ..Default::default() }).unwrap();
    let n = p.len();
    p.insert_text(n, "INSERTED", &CharProps { ins: Some(1), ..Default::default() }).unwrap();
    d.body = vec![para_block(p)];
    // The default export shows the final text, as Word's "No Markup" does.
    let text = squash(&extract_text(&export(&d, &PdfOptions::default()).unwrap()).concat());
    assert_eq!(text, "Keep INSERTED");
    // With markup the deletion is still there (struck through).
    let text = squash(&extract_text(&export(&d, &PdfOptions { include_markup: true, ..Default::default() }).unwrap()).concat());
    assert_eq!(text, "Keep DELETEDTEXT INSERTED");
}

#[test]
fn deleted_heading_text_stays_out_of_bookmarks_and_tags() {
    use wordcraft_doc::{Revision, RevisionKind};
    let mut d = Document::new();
    d.revisions.push(Revision { kind: RevisionKind::Delete, author: "Ada".into(), date: String::new() });
    let del = CharProps { del: Some(0), ..Default::default() };
    // "Intro SECRET Part" with "SECRET " deleted, then a heading deleted as a whole.
    let mut h = Paragraph::with_text("Intro ", CharProps::default()).styled("Heading1");
    let n = h.len();
    h.insert_text(n, "SECRET ", &del).unwrap();
    let n = h.len();
    h.insert_text(n, "Part", &CharProps::default()).unwrap();
    let gone = Paragraph::with_text("GONE", del.clone()).styled("Heading1");
    d.body = vec![para_block(h), para_block(Paragraph::with_text("Body text", CharProps::default())), para_block(gone)];
    let raw = |opts: PdfOptions| String::from_utf8_lossy(&export(&d, &PdfOptions { compress: false, ..opts }).unwrap()).into_owned();
    // Bookmarks alone (untagged), then bookmarks and heading tags: the final text only.
    for tagged in [false, true] {
        let pdf = raw(PdfOptions { tagged, ..Default::default() });
        assert!(!pdf.contains("SECRET") && !pdf.contains("GONE"), "tagged {tagged}: deleted heading text in the PDF");
        assert!(pdf.contains("/Outlines") && pdf.contains("Intro Part"), "tagged {tagged}: no bookmark for the heading");
    }
    // With markup the deleted text is shown, and so it is in the bookmarks.
    let pdf = raw(PdfOptions { include_markup: true, ..Default::default() });
    assert!(pdf.contains("Intro SECRET Part") && pdf.contains("GONE"));
}

#[test]
fn hidden_heading_text_stays_out_of_bookmarks_and_tags() {
    let mut d = Document::new();
    // Hidden text both directly and through a character style.
    d.styles.upsert(wordcraft_doc::Style {
        id: "Secret".into(),
        name: "Secret".into(),
        kind: wordcraft_doc::StyleKind::Character,
        chr: CharProps { hidden: Some(true), ..Default::default() },
        ..Default::default()
    });
    let hidden = CharProps { hidden: Some(true), ..Default::default() };
    // "Intro SECRET Part" with "SECRET " hidden, a heading hidden as a whole, and one hidden by style.
    let mut h = Paragraph::with_text("Intro ", CharProps::default()).styled("Heading1");
    let n = h.len();
    h.insert_text(n, "SECRET ", &hidden).unwrap();
    let n = h.len();
    h.insert_text(n, "Part", &CharProps::default()).unwrap();
    let gone = Paragraph::with_text("GONE", hidden.clone()).styled("Heading1");
    let styled = Paragraph::with_text("STYLED", CharProps { style: Some("Secret".into()), ..Default::default() }).styled("Heading1");
    d.body = vec![
        para_block(h),
        para_block(Paragraph::with_text("Body text", CharProps::default())),
        para_block(gone),
        para_block(styled),
        para_block(Paragraph::with_text("More text", CharProps::default())),
    ];
    let raw = |opts: PdfOptions| String::from_utf8_lossy(&export(&d, &PdfOptions { compress: false, ..opts }).unwrap()).into_owned();
    // The PDF never prints hidden text, with or without markup, so neither do bookmarks or tags.
    for (tagged, include_markup) in [(false, false), (true, false), (true, true)] {
        let pdf = raw(PdfOptions { tagged, include_markup, ..Default::default() });
        let what = format!("tagged {tagged} markup {include_markup}");
        assert!(!pdf.contains("SECRET") && !pdf.contains("GONE") && !pdf.contains("STYLED"), "{what}: hidden heading text in the PDF");
        assert!(pdf.contains("/Outlines") && pdf.contains("Intro Part"), "{what}: no bookmark for the heading");
    }
}

#[test]
fn long_fragmented_hidden_headings_export_in_linear_time() {
    // Headings of n chars in one-char runs: all hidden with bold alternating, and hidden and visible
    // alternating. Layout and the bookmark/tag titles once scanned every left-out range for every
    // char, quadratic in n. Machine-independent: the time for 4n against n, linear ~4, quadratic ~16.
    let doc = |n: usize| {
        let heading = |visible_every_other: bool| {
            let mut h = Paragraph::with_text(&"abcdefg ".repeat(n / 8), CharProps::default()).styled("Heading1");
            h.runs = (0..n)
                .map(|i| {
                    let hidden = !(visible_every_other && i % 2 == 1);
                    wordcraft_doc::Run { len: 1, props: CharProps { hidden: Some(hidden), bold: Some(i % 4 < 2), ..Default::default() } }
                })
                .collect();
            para_block(h)
        };
        let mut d = Document::new();
        d.body = vec![heading(false), heading(true), para_block(Paragraph::with_text("After", CharProps::default()))];
        d
    };
    let opts = PdfOptions { compress: false, ..Default::default() };
    // The fastest of a few runs, after a warm-up, so one slow run on a busy machine counts less.
    let fastest = |d: &Document| {
        (0..3)
            .map(|_| {
                let t = std::time::Instant::now();
                export(d, &opts).unwrap();
                t.elapsed().as_secs_f64()
            })
            .fold(f64::INFINITY, f64::min)
    };
    let n = 25_000;
    let (small, big) = (doc(n), doc(4 * n));
    export(&small, &opts).unwrap();
    let (t1, t4) = (fastest(&small), fastest(&big));
    let ratio = t4 / t1.max(1e-9);
    eprintln!("t(n) {t1:.3} s, t(4n) {t4:.3} s, ratio {ratio:.1}");
    assert!(ratio < 8.0, "4x the text took {ratio:.1}x the time ({t1:.3} s → {t4:.3} s): not linear");
    let pdf = String::from_utf8_lossy(&export(&big, &opts).unwrap()).into_owned();
    // The all-hidden heading has no bookmark; the other is titled with its visible chars only.
    assert!(!pdf.contains("abcd"), "hidden heading text in a title");
    assert!(pdf.contains("bdf bdf"), "no title for the half-visible heading");
}

#[test]
fn table_style_hidden_heading_text_stays_out_of_bookmarks_and_tags() {
    // Hidden by the table style's header row, so only the layout's resolution (table conditional
    // formatting under the runs' own) says the text is hidden.
    let mut d = Document::new();
    d.styles.upsert(wordcraft_doc::Style {
        id: "SecretHeader".into(),
        name: "Secret Header".into(),
        kind: wordcraft_doc::StyleKind::Table,
        table: Some(wordcraft_doc::styles::TableStyleParts {
            header_chr: CharProps { hidden: Some(true), ..Default::default() },
            ..Default::default()
        }),
        ..Default::default()
    });
    let mut t = Table::new(2, 2, 400.0);
    t.props.style = Some("SecretHeader".into());
    // Header row: a heading hidden as a whole, and "Shown SECRET" where "Shown " is unhidden directly.
    let mut partly = Paragraph::with_text("Shown ", CharProps { hidden: Some(false), ..Default::default() }).styled("Heading1");
    let n = partly.len();
    partly.insert_text(n, "SECRET", &CharProps::default()).unwrap();
    t.rows[0].cells[0].blocks = vec![para_block(Paragraph::with_text("GONE", CharProps::default()).styled("Heading1"))];
    t.rows[0].cells[1].blocks = vec![para_block(partly)];
    t.rows[1].cells[0].blocks = vec![para_block(Paragraph::with_text("Body cell", CharProps::default()))];
    d.body = vec![para_block(Paragraph::with_text("Before", CharProps::default())), Arc::new(Block::Table(t))];
    let raw = |opts: PdfOptions| String::from_utf8_lossy(&export(&d, &PdfOptions { compress: false, ..opts }).unwrap()).into_owned();
    for (tagged, include_markup) in [(false, false), (true, false), (true, true)] {
        let pdf = raw(PdfOptions { tagged, include_markup, ..Default::default() });
        let what = format!("tagged {tagged} markup {include_markup}");
        assert!(!pdf.contains("SECRET") && !pdf.contains("GONE"), "{what}: hidden table heading text in the PDF");
        assert!(pdf.contains("/Outlines") && pdf.contains("Shown"), "{what}: no bookmark for the visible part");
    }
}

#[test]
fn text_mapping_handles_ligatures_and_extras() {
    let d = Document::from_text("office affine");
    let lay = layout(&d, &mut LayoutCache::new(), &LayoutOptions::default());
    let bytes = export_layout(&d, &lay, &PdfOptions::default()).unwrap();
    assert!(squash(&extract_text(&bytes)[0]).contains("office affine"));
}

#[test]
fn shapes_have_paths() {
    let r = Rect::new(0.0, 0.0, 10.0, 10.0);
    for k in [
        ShapeKind::Rectangle,
        ShapeKind::RoundedRectangle,
        ShapeKind::Ellipse,
        ShapeKind::Star,
        ShapeKind::Heart,
        ShapeKind::Arrow,
        ShapeKind::Line,
        ShapeKind::Diamond,
        ShapeKind::Triangle,
    ] {
        assert!(shape_path(k, &r).is_some(), "{k:?}");
    }
    assert!(shape_path(ShapeKind::Ellipse, &Rect::new(0.0, 0.0, f32::NAN, 1.0)).is_none());
    assert_eq!(parse_iso("2024-02-03T04:05:06Z"), Some((2024, 2, 3, 4, 5, 6)));
    assert_eq!(parse_iso("garbage"), None);
    assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
}
