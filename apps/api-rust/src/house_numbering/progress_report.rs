//! Bounded, dependency-free OOXML progress report. Only typed numbers/booleans
//! and inline strings are emitted; untrusted text is never a spreadsheet formula.

use std::fmt::Write as _;

use ovg_core::AppError;
use serde_json::Value;

const MAX_WORKBOOK_BYTES: usize = 32 * 1024 * 1024;
const SUMMARY: [&str; 7] = [
    "positions",
    "houses",
    "vacant",
    "other",
    "completion",
    "removed",
    "submitted_pending",
];
const COLUMNS: [&str; 6] = [
    "building_id",
    "route_name",
    "house_number",
    "condition",
    "state",
    "updated_at",
];

fn limit() -> AppError {
    AppError::validation("REPORT_SIZE_EXCEEDED", "Progress workbook exceeds safe byte limit.")
}

fn xml_text(value: &str) -> String {
    let mut result = String::new();
    if value.starts_with(&['=', '+', '-', '@'][..]) {
        result.push('\'');
    }
    for c in value.chars().take(32_766) {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '\t' | '\n' | '\r' => result.push(c),
            c if c >= '\u{20}' && c != '\u{fffe}' && c != '\u{ffff}' => result.push(c),
            _ => result.push('\u{fffd}'),
        }
    }
    result
}

fn cell(sheet: &mut String, col: usize, row: usize, value: &Value) {
    let col = ["A", "B", "C", "D", "E", "F"][col];
    match value {
        Value::Bool(flag) => {
            let _ = write!(sheet, "<c r=\"{col}{row}\" t=\"b\"><v>{}</v></c>", u8::from(*flag));
        }
        Value::Number(number) => {
            let _ = write!(sheet, "<c r=\"{col}{row}\"><v>{number}</v></c>");
        }
        _ => {
            let text = xml_text(value.as_str().unwrap_or(""));
            let _ = write!(
                sheet,
                "<c r=\"{col}{row}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{text}</t></is></c>"
            );
        }
    }
}

fn row(sheet: &mut String, index: usize, values: &[Value]) -> Result<(), AppError> {
    let _ = write!(sheet, "<row r=\"{index}\">");
    for (col, value) in values.iter().enumerate() {
        cell(sheet, col, index, value);
    }
    sheet.push_str("</row>");
    if sheet.len() > MAX_WORKBOOK_BYTES - 16_384 {
        return Err(limit());
    }
    Ok(())
}

fn u16le(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn u32le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// ZIP stored entries, valid for OPC/OOXML without requiring a compression crate.
fn zip(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, AppError> {
    let mut body = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let name = name.as_bytes();
        let name_len = u16::try_from(name.len()).map_err(|_| limit())?;
        let size = u32::try_from(data.len()).map_err(|_| limit())?;
        let offset = u32::try_from(body.len()).map_err(|_| limit())?;
        let crc = crc32fast::hash(data);
        u32le(&mut body, 0x0403_4b50);
        for value in [20, 0, 0, 0, 33] {
            u16le(&mut body, value);
        }
        for value in [crc, size, size] {
            u32le(&mut body, value);
        }
        u16le(&mut body, name_len);
        u16le(&mut body, 0);
        body.extend_from_slice(name);
        body.extend_from_slice(data);

        u32le(&mut central, 0x0201_4b50);
        for value in [20, 20, 0, 0, 0, 33] {
            u16le(&mut central, value);
        }
        for value in [crc, size, size] {
            u32le(&mut central, value);
        }
        for value in [name_len, 0, 0, 0, 0] {
            u16le(&mut central, value);
        }
        u32le(&mut central, 0);
        u32le(&mut central, offset);
        central.extend_from_slice(name);
    }
    let start = u32::try_from(body.len()).map_err(|_| limit())?;
    let size = u32::try_from(central.len()).map_err(|_| limit())?;
    let count = u16::try_from(entries.len()).map_err(|_| limit())?;
    body.extend_from_slice(&central);
    u32le(&mut body, 0x0605_4b50);
    for value in [0, 0, count, count] {
        u16le(&mut body, value);
    }
    u32le(&mut body, size);
    u32le(&mut body, start);
    u16le(&mut body, 0);
    if body.len() > MAX_WORKBOOK_BYTES {
        return Err(limit());
    }
    Ok(body)
}

pub(super) fn build(summary: &Value, rows: &[Value]) -> Result<Vec<u8>, AppError> {
    let mut sheet = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>",
    );
    row(&mut sheet, 1, &[Value::from("Chỉ số"), Value::from("Giá trị")])?;
    for (index, key) in SUMMARY.iter().enumerate() {
        row(&mut sheet, index + 2, &[Value::from(*key), summary[*key].clone()])?;
    }
    row(&mut sheet, 9, &[])?;
    row(&mut sheet, 10, &COLUMNS.map(Value::from))?;
    for (index, item) in rows.iter().enumerate() {
        row(&mut sheet, index + 11, &COLUMNS.map(|key| item[key].clone()))?;
    }
    sheet.push_str("</sheetData></worksheet>");
    let content_types = br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
    let root_rels = br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;
    let workbook = r#"<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Tiến độ" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let workbook_rels = br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;
    zip(&[
        ("[Content_Types].xml", content_types),
        ("_rels/.rels", root_rels),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", workbook_rels),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_strings_escape_formulas_and_xml() {
        assert_eq!(xml_text("=1+2"), "'=1+2");
        assert_eq!(xml_text("Đường A & B < 2"), "Đường A &amp; B &lt; 2");
        assert_eq!(xml_text("a\0b"), "a\u{fffd}b");
    }

    #[test]
    fn workbook_has_zip_end_record() {
        let bytes = build(&serde_json::json!({"positions": 2}), &[]).unwrap();
        assert!(bytes.starts_with(b"PK\x03\x04"));
        assert_eq!(&bytes[bytes.len() - 22..bytes.len() - 18], b"PK\x05\x06");
    }
}
