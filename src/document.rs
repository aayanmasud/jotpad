use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Loads UTF-8 text files, or extracts paragraph text from a DOCX document.
pub fn load(path: &Path) -> Result<String, String> {
    if is_docx(path) {
        load_docx(path)
    } else {
        std::fs::read_to_string(path).map_err(|error| format!("Could not open file: {error}"))
    }
}

/// Saves plain text directly, or creates a text-focused DOCX with one paragraph per line.
pub fn save(path: &Path, text: &str) -> Result<(), String> {
    if is_docx(path) {
        save_docx(path, text)
    } else {
        std::fs::write(path, text).map_err(|error| format!("Could not save file: {error}"))
    }
}

fn is_docx(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("docx"))
}

fn load_docx(path: &Path) -> Result<String, String> {
    let file = File::open(path).map_err(|error| format!("Could not open DOCX file: {error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("Invalid DOCX archive: {error}"))?;
    let mut document_xml = String::new();
    archive
        .by_name("word/document.xml")
        .map_err(|error| format!("DOCX is missing its main document: {error}"))?
        .read_to_string(&mut document_xml)
        .map_err(|error| format!("Could not read DOCX document: {error}"))?;

    extract_paragraph_text(&document_xml)
}

fn extract_paragraph_text(xml: &str) -> Result<String, String> {
    let document = roxmltree::Document::parse(xml)
        .map_err(|error| format!("Could not parse DOCX document XML: {error}"))?;
    let mut paragraphs = Vec::new();

    for paragraph in document
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "p")
    {
        let mut text = String::new();
        for node in paragraph.descendants().filter(|node| node.is_element()) {
            match node.tag_name().name() {
                "t" => {
                    if let Some(value) = node.text() {
                        text.push_str(value);
                    }
                }
                "tab" => text.push('\t'),
                "br" | "cr" => text.push('\n'),
                _ => {}
            }
        }
        paragraphs.push(text);
    }

    Ok(paragraphs.join("\n"))
}

fn save_docx(path: &Path, text: &str) -> Result<(), String> {
    use docx_rs::{Docx, Paragraph, Run};

    let file = File::create(path).map_err(|error| format!("Could not create DOCX file: {error}"))?;
    let mut document = Docx::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        document = document.add_paragraph(Paragraph::new().add_run(Run::new().add_text(line)));
    }
    document
        .build()
        .pack(file)
        .map_err(|error| format!("Could not write DOCX file: {error}"))
}

#[cfg(test)]
mod tests {
    use super::extract_paragraph_text;

    #[test]
    fn extracts_paragraphs_tabs_and_line_breaks() {
        let xml = r#"<?xml version="1.0"?><w:document xmlns:w="urn:test"><w:body><w:p><w:r><w:t>Hello &amp; welcome</w:t></w:r></w:p><w:p><w:r><w:t>One</w:t><w:tab/><w:t>Two</w:t><w:br/><w:t>Three</w:t></w:r></w:p></w:body></w:document>"#;
        assert_eq!(
            extract_paragraph_text(xml).unwrap(),
            "Hello & welcome\nOne\tTwo\nThree"
        );
    }

    #[test]
    fn preserves_empty_paragraphs() {
        let xml = r#"<w:document xmlns:w="urn:test"><w:body><w:p><w:r><w:t>Before</w:t></w:r></w:p><w:p/><w:p><w:r><w:t>After</w:t></w:r></w:p></w:body></w:document>"#;
        assert_eq!(extract_paragraph_text(xml).unwrap(), "Before\n\nAfter");
    }
}
