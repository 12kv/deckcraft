//! Raw OOXML paragraph defaults must survive import, resolution and export.
use deckcraft_model::text::{LevelStyle, Paragraph, RunKind, RunProps};
use deckcraft_model::{Presentation, defaults, edit, resolve};
use std::io::{Cursor, Read, Write};

fn imported() -> Presentation {
    let bytes = deckcraft_pptx::export(&defaults::new_presentation(None)).unwrap();
    let mut input = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..input.len() {
        let mut file = input.by_index(i).unwrap();
        let name = file.name().to_string();
        let mut data = Vec::new();
        file.read_to_end(&mut data).unwrap();
        if name == "ppt/slides/slide1.xml" {
            let mut xml = String::from_utf8(data).unwrap();
            let start = xml.find("<p:txBody>").unwrap();
            let end = start + xml[start..].find("</p:txBody>").unwrap() + "</p:txBody>".len();
            xml.replace_range(start..end, r#"<p:txBody><a:bodyPr/><a:lstStyle><a:lvl1pPr><a:defRPr sz="1400" i="1"/></a:lvl1pPr></a:lstStyle>
<a:p><a:pPr><a:defRPr sz="2800" b="1"><a:solidFill><a:srgbClr val="123456"/></a:solidFill><a:latin typeface="Inter"/><a:ea typeface="BIZ UDPGothic"/></a:defRPr></a:pPr>
<a:r><a:t>Inherited</a:t></a:r><a:r><a:rPr b="0" i="0"/><a:t>Not bold</a:t></a:r>
<a:br/><a:fld id="field" type="slidenum"><a:rPr sz="1600"/><a:t>1</a:t></a:fld></a:p>
<a:p><a:pPr><a:defRPr sz="2400"/></a:pPr><a:endParaRPr i="0"/></a:p>
<a:p><a:r><a:t>No paragraph override</a:t></a:r></a:p></p:txBody>"#);
            data = xml.into_bytes();
        }
        output.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        output.write_all(&data).unwrap();
    }
    deckcraft_pptx::import(&output.finish().unwrap().into_inner()).unwrap()
}

fn check(pres: &Presentation) {
    let slide = &pres.slides[0];
    let shape = &slide.shapes[0];
    let ctx = resolve::Ctx::for_slide(pres, slide).unwrap();
    let paras = &shape.text.as_ref().unwrap().paragraphs;
    let first = &paras[0];
    assert_eq!(first.default_run_props.size, Some(28.0));
    // Defaults are kept separately; explicit false values must not be lost or flattened.
    assert_eq!(first.runs[0].props, RunProps::default());
    let props = resolve::run(&ctx, shape, first, &first.runs[0].props);
    assert_eq!(props.size, Some(28.0));
    assert_eq!(props.bold, Some(true));
    assert_eq!(props.italic, Some(true)); // lower-priority list default still applies
    assert_eq!(props.font.as_deref(), Some("Inter"));
    assert_eq!(props.font_ea.as_deref(), Some("BIZ UDPGothic"));
    assert_eq!(props.fill, first.default_run_props.fill);
    let override_props = resolve::run(&ctx, shape, first, &first.runs[1].props);
    assert_eq!(override_props.bold, Some(false));
    assert_eq!(override_props.italic, Some(false));
    assert_eq!(override_props.size, Some(28.0));
    assert_eq!(override_props.font, props.font);
    assert!(matches!(first.runs[2].kind, RunKind::Break));
    assert_eq!(resolve::run(&ctx, shape, first, &first.runs[2].props).size, Some(28.0));
    assert_eq!(resolve::run(&ctx, shape, first, &first.runs[3].props).size, Some(16.0));
    let empty = &paras[1];
    let caret = resolve::run(&ctx, shape, empty, &empty.end_props);
    assert_eq!(caret.size, Some(24.0));
    assert_eq!(caret.italic, Some(false));
    assert_eq!(resolve::run(&ctx, shape, &paras[2], &paras[2].runs[0].props).size, Some(14.0));
}

#[test]
fn paragraph_defaults_inherit_per_property_without_leaking_to_other_paragraphs() {
    check(&imported());
}

#[test]
fn paragraph_defaults_round_trip_separately_from_explicit_runs() {
    let original = imported();
    let bytes = deckcraft_pptx::export(&original).unwrap();
    check(&deckcraft_pptx::import(&bytes).unwrap());
}

#[test]
fn missing_properties_still_inherit_from_the_master() {
    let mut p = imported();
    let master = std::sync::Arc::make_mut(&mut p.masters[0]);
    let mut style = master.title_style.level(0).cloned().unwrap_or_default();
    style.run.spacing = Some(2.5);
    master.title_style.set(0, LevelStyle { run: style.run, para: style.para });
    let s = &p.slides[0].shapes[0];
    let para = &s.text.as_ref().unwrap().paragraphs[0];
    let ctx = resolve::Ctx::for_slide(&p, &p.slides[0]).unwrap();
    let run = resolve::run(&ctx, s, para, &para.runs[0].props);
    assert_eq!(run.spacing, Some(2.5));
    assert_eq!(run.size, Some(28.0));
}

#[test]
fn splitting_a_paragraph_preserves_its_default_run_style() {
    let p = imported();
    let mut body = p.slides[0].shapes[0].text.clone().unwrap();
    let defaults = body.paragraphs[0].default_run_props.clone();
    edit::insert(&mut body, (0, 3), "\n", None);
    assert_eq!(body.paragraphs[0].default_run_props, defaults);
    assert_eq!(body.paragraphs[1].default_run_props, defaults);
}

#[test]
fn old_serialized_paragraphs_default_to_no_run_override() {
    let p: Paragraph = serde_json::from_str(r#"{"runs":[]}"#).unwrap();
    assert_eq!(p.default_run_props, RunProps::default());
    assert!(!serde_json::to_string(&p).unwrap().contains("defaultRunProps"));
}
