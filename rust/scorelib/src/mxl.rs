//! MXL file handler — reads compressed MusicXML (.mxl) archives.
//!
//! An .mxl file is a ZIP archive containing:
//!   - META-INF/container.xml  — declares the root MusicXML file path
//!   - <rootfile>.xml          — the actual MusicXML content (e.g., score.xml)
//!   - (optional) other files  — images, sounds, etc.

use std::io::{Cursor, Read};
use zip::ZipArchive;

use crate::model::Score;
use crate::parser;

/// Read and parse a .mxl file from raw bytes.
pub fn parse_mxl(data: &[u8]) -> Result<Score, String> {
    let xml = extract_musicxml_from_mxl(data)?;
    parser::parse_musicxml(&xml)
}

/// Extract the MusicXML content string from .mxl bytes.
pub fn extract_musicxml_from_mxl(data: &[u8]) -> Result<String, String> {
    let cursor = Cursor::new(data);
    let mut archive =
        ZipArchive::new(cursor).map_err(|e| format!("Failed to open MXL archive: {e}"))?;

    // Step 1: Read container.xml to find the root MusicXML file
    let root_file_path = read_container_xml(&mut archive)?;

    // Step 2: Read the root MusicXML file
    let mut root_file = archive
        .by_name(&root_file_path)
        .map_err(|e| format!("Root file '{root_file_path}' not found in archive: {e}"))?;

    let mut xml = String::new();
    root_file
        .read_to_string(&mut xml)
        .map_err(|e| format!("Failed to read '{root_file_path}': {e}"))?;

    Ok(xml)
}

/// Parse META-INF/container.xml to find the root MusicXML file path.
fn read_container_xml(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Result<String, String> {
    // First, try to read container.xml
    let container_xml = {
        match archive.by_name("META-INF/container.xml") {
            Ok(mut container_file) => {
                let mut xml = String::new();
                container_file
                    .read_to_string(&mut xml)
                    .map_err(|e| format!("Failed to read container.xml: {e}"))?;
                Some(xml)
            }
            Err(_) => None,
        }
    }; // mutable borrow of archive is released here

    // If we got container.xml, parse it for the rootfile path
    if let Some(xml) = container_xml {
        let doc = roxmltree::Document::parse(&xml)
            .map_err(|e| format!("Failed to parse container.xml: {e}"))?;

        for node in doc.descendants() {
            if node.tag_name().name() == "rootfile" {
                if let Some(path) = node.attribute("full-path") {
                    return Ok(path.to_string());
                }
            }
        }

        return Err("No rootfile found in container.xml".to_string());
    }

    // Fallback: look for common MusicXML filenames in the archive
    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();

    // Look for .xml or .musicxml files (not in META-INF)
    for name in &names {
        if !name.starts_with("META-INF/")
            && (name.ends_with(".xml") || name.ends_with(".musicxml"))
        {
            return Ok(name.clone());
        }
    }

    Err(format!(
        "No MusicXML file found in archive. Files: {:?}",
        names
    ))
}
