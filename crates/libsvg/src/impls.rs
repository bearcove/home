use std::collections::{HashMap, HashSet};

use camino::Utf8PathBuf;
use config_types::FontStyle;
use conflux::{Dimensions, SvgFontFaceCollection};
use image_types::{IntrinsicPixels, PixelDensity};

use crate::{SvgCleanupOptions, char_usage};

macro_rules! trace {
    ($($arg:tt)*) => {
        eprintln!("\x1b[32mmod-svg:\x1b[0m {}", format!($($arg)*));
    };
}

pub async fn inject_font_faces(
    input: &[u8],
    font_faces: &SvgFontFaceCollection,
) -> eyre::Result<Vec<u8>> {
    use quick_xml::{
        Reader, Writer,
        events::{BytesText, Event},
    };

    let chars_per_typo = crate::char_usage::analyze_char_usage(input)?;

    let mut subsetter = FontSubsetter::new()?;

    for face in &font_faces.faces {
        let font_full_name = face.full_name();
        subsetter
            .add_font(
                font_full_name,
                face.file_name.as_str(),
                face.contents.clone(),
            )
            .await?;
    }

    let mut reader = Reader::from_reader(input);
    let mut writer = Writer::new(Vec::new());
    let mut wrote_font_faces = false;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) if e.name().as_ref() == b"style" => {
                writer.write_event(Event::Start(e))?;
                if !wrote_font_faces {
                    let mut content = String::new();
                    for face in &font_faces.faces {
                        let font_full_name = face.full_name();
                        use std::fmt::Write;
                        let used_chars = chars_per_typo
                            .get(&char_usage::Typo {
                                family: Some(face.family.clone()),
                                weight: Some(face.weight),
                                style: Some(FontStyle::Normal),
                            })
                            .cloned()
                            .unwrap_or_default();
                        trace!(
                            "Subsetting font {} with {} used chars: {:?}",
                            font_full_name,
                            used_chars.len(),
                            used_chars
                        );

                        let subset_data = subsetter.subset(&font_full_name, used_chars).await?;
                        write!(
                            &mut content,
                            "@font-face{{font-family:{};{}src:url(data:font/woff2;base64,{})}}",
                            face.family,
                            face.weight.as_css_prop(),
                            base64::Engine::encode(
                                &base64::engine::general_purpose::STANDARD,
                                &subset_data
                            )
                        )?;
                    }
                    writer.write_event(Event::Text(BytesText::from_escaped(&content)))?;
                    wrote_font_faces = true;
                }
            }
            Ok(Event::Eof) => break,
            Ok(event) => {
                writer.write_event(event)?;
            }
            Err(e) => return Err(e.into()),
        }
    }

    Ok(writer.into_inner())
}

pub(crate) fn cleanup_svg(input: &[u8], _opts: SvgCleanupOptions) -> eyre::Result<Vec<u8>> {
    use quick_xml::{Reader, Writer, events::Event};

    let mut reader = Reader::from_reader(input);
    let mut writer = Writer::new(Vec::new());
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"svg" => {
                let mut width = None;
                let mut height = None;
                let mut has_viewbox = false;
                let mut new_elem = e.clone();

                // Check for width/height/viewBox
                for attr in new_elem.attributes() {
                    let attr = attr?;
                    match attr.key.as_ref() {
                        b"width" => {
                            width = Some(std::str::from_utf8(&attr.value)?.to_string());
                        }
                        b"height" => {
                            height = Some(std::str::from_utf8(&attr.value)?.to_string());
                        }
                        b"viewBox" => {
                            has_viewbox = true;
                        }
                        _ => {}
                    }
                }

                // Add viewBox if we have width/height but no viewBox
                if let (Some(width_val), Some(height_val)) = (width, height) {
                    if !has_viewbox {
                        new_elem.push_attribute((
                            "viewBox",
                            format!("0 0 {width_val} {height_val}").as_str(),
                        ));
                    }
                }

                writer.write_event(Event::Start(new_elem))?;
            }
            Ok(Event::Eof) => break,
            Ok(event) => writer.write_event(event)?,
            Err(e) => return Err(e.into()),
        }
        buf.clear();
    }

    Ok(writer.into_inner())
}

struct FontSubsetter {
    tmp_dir: tempfile::TempDir,
    known_fonts: HashMap<String, Utf8PathBuf>,
}

impl FontSubsetter {
    fn new() -> eyre::Result<Self> {
        let tmp_dir = tempfile::TempDir::new()?;
        Ok(FontSubsetter {
            tmp_dir,
            known_fonts: HashMap::new(),
        })
    }

    /// `font_file_name` is supposed to be something like `Iosevka-Regular.woff2`
    /// and `font_name` is supposed to be `Iosevka`, something that's used in the SVG itself
    async fn add_font(
        &mut self,
        font_name: String,
        font_file_name: &str,
        data: Vec<u8>,
    ) -> eyre::Result<()> {
        trace!("Adding font {} from {}", font_name, font_file_name);
        if let Some(old_path) = self.known_fonts.remove(&font_name) {
            let _ = tokio::fs::remove_file(old_path).await;
        }

        let font_path = self.tmp_dir.path().join(font_file_name);
        trace!("Writing font to {}", font_path.display());
        tokio::fs::write(&font_path, &data).await?;
        self.known_fonts.insert(font_name, font_path.try_into()?);
        Ok(())
    }

    async fn subset(
        &mut self,
        font_name: &str,
        used_chars: HashSet<char>,
    ) -> eyre::Result<Vec<u8>> {
        let font_path = self
            .known_fonts
            .get(font_name)
            .ok_or(eyre::eyre!("unknown font {font_name}"))?;
        trace!("Subsetting font {}", font_name);
        if tokio::fs::metadata(font_path).await.is_err() {
            return Err(eyre::eyre!("could not find font file?"));
        }

        let subset_path = self
            .tmp_dir
            .path()
            .join(format!("{font_name}.subset.woff2"));

        let pyftsubset_path = which::which("pyftsubset")?;

        let mut child = tokio::process::Command::new(pyftsubset_path)
            .arg(font_path.as_str())
            .arg(format!("--text={}", used_chars.iter().collect::<String>()))
            .arg("--flavor=woff2")
            .arg(format!("--output-file={}", subset_path.display()))
            .arg("--no-hinting")
            .arg("--desubroutinize")
            .arg("--no-notdef-outline")
            .arg("--name-IDs=")
            .arg("--no-name-legacy")
            .spawn()?;

        let status = child.wait().await?;
        if !status.success() {
            return Err(eyre::eyre!("pyftsubset failed with status: {}", status));
        }

        let output = tokio::fs::read(&subset_path).await;
        let _ = tokio::fs::remove_file(&subset_path).await;
        Ok(output?)
    }
}

pub fn svg_dimensions(input: &[u8]) -> Option<Dimensions> {
    let mut reader = quick_xml::Reader::from_reader(input);
    let mut buf = Vec::new();

    // Look for SVG root element to get viewport dimensions
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Start(e)) if e.name().as_ref() == b"svg" => {
                // Parse width and height attributes
                let mut width = None;
                let mut height = None;
                let mut min_x = 0.0;
                let mut min_y = 0.0;

                for attr in e.attributes() {
                    let attr = match attr {
                        Ok(attr) => attr,
                        Err(e) => {
                            trace!("Error reading SVG attribute: {e}");
                            continue;
                        }
                    };

                    match attr.key.as_ref() {
                        b"width" => {
                            width = std::str::from_utf8(&attr.value).ok()?.parse::<f64>().ok();
                        }
                        b"height" => {
                            height = std::str::from_utf8(&attr.value).ok()?.parse::<f64>().ok();
                        }
                        b"viewBox" => {
                            // Parse viewBox="minX minY width height"
                            if let Ok(vb) = std::str::from_utf8(&attr.value) {
                                let parts: Vec<f64> = vb
                                    .split_whitespace()
                                    .filter_map(|s| s.parse::<f64>().ok())
                                    .collect();
                                if parts.len() == 4 {
                                    min_x = parts[0];
                                    min_y = parts[1];
                                    // Only use viewBox dimensions if width/height not explicitly set
                                    if width.is_none() {
                                        width = Some(parts[2]);
                                    }
                                    if height.is_none() {
                                        height = Some(parts[3]);
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }

                if let (Some(w), Some(h)) = (width, height) {
                    return Some(Dimensions {
                        w: IntrinsicPixels::from((w - min_x).abs() as u32),
                        h: IntrinsicPixels::from((h - min_y).abs() as u32),
                        density: PixelDensity::ONE, // not really meaningful for SVG
                    });
                }

                break;
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod test {
    use base64::Engine;
    use config_types::{FontStyle, FontWeight};
    use conflux::{InputHash, SvgFontFace};
    use tokio::io::AsyncBufReadExt;

    use super::*;

    #[test]
    fn test_svg_dimensions() {
        let svg_bytes = include_bytes!("testdata/extend.svg");
        let dimensions = svg_dimensions(svg_bytes).unwrap();

        assert_eq!(
            (dimensions.w, dimensions.h),
            (IntrinsicPixels::from(361), IntrinsicPixels::from(162))
        );
    }

    #[test]
    fn test_cleanup_svg() {
        let svg_bytes = include_bytes!("testdata/no-viewbox.svg");
        let cleaned_svg = cleanup_svg(svg_bytes, SvgCleanupOptions {}).unwrap();
        insta::assert_snapshot!(String::from_utf8_lossy(&cleaned_svg));
    }

    #[tokio::test]
    async fn test_subset_font() {
        use tokio::io::BufReader;
        use tokio::process::Command;

        let included_chars = "abcdefBOLDitalic";
        let font_bytes = include_bytes!("testdata/iosevka-regular.woff2");

        let input_font_path = "/tmp/input.woff2";
        let output_font_path = "/tmp/blah.woff2";

        tokio::fs::write(input_font_path, font_bytes)
            .await
            .expect("Failed to write input font");

        let pyftsubset_path = which::which("pyftsubset")
            .expect("pyftsubset not found - please install fonttools with: brew install fonttools");

        let mut child = Command::new(pyftsubset_path)
            .arg(input_font_path)
            .arg(format!("--text={included_chars}"))
            .arg("--flavor=woff2")
            .arg(format!("--output-file={output_font_path}"))
            .arg("--no-hinting")
            .arg("--desubroutinize")
            .arg("--no-notdef-outline")
            .arg("--name-IDs=")
            .arg("--no-name-legacy")
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("Failed to start pyftsubset");

        let stderr_handle = child.stderr.take().expect("Failed to open stderr");

        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr_handle);
            let mut line = String::new();
            while reader
                .read_line(&mut line)
                .await
                .expect("Failed to read line")
                > 0
            {
                trace!("{}", line);
                line.clear();
            }
        });

        stderr_task.await.expect("Failed to join stderr task");

        let status = child.wait().await.expect("Failed to wait for pyftsubset");
        assert!(
            status.success(),
            "pyftsubset failed with status: {status}"
        );

        // Verify the output file exists and has contents
        assert!(std::path::Path::new(output_font_path).exists());
    }

    #[tokio::test]
    async fn test_inject_font_faces() {
        let svg_bytes = include_bytes!("testdata/drawio-bold-test.svg");

        let font_face_regular = SvgFontFace {
            family: "IosevkaFtl".to_string(),
            weight: FontWeight(400),
            style: FontStyle::Normal,
            file_name: "iosevka-regular.woff2".to_string(),
            contents: include_bytes!("testdata/iosevka-regular.woff2").to_vec(),
            hash: InputHash::from_static("dummy1"),
        };
        let font_face_bold = SvgFontFace {
            family: "IosevkaFtl".to_string(),
            weight: FontWeight(700),
            style: FontStyle::Normal,
            file_name: "iosevka-bold.woff2".to_string(),
            contents: include_bytes!("testdata/iosevka-bold.woff2").to_vec(),
            hash: InputHash::from_static("dummy2"),
        };
        let font_faces = SvgFontFaceCollection {
            faces: vec![font_face_regular, font_face_bold],
        };

        let result = inject_font_faces(&svg_bytes[..], &font_faces).await;

        let modified_svg = result.expect("inject_font_faces failed");
        // Write the modified SVG to a temporary file for inspection
        let temp_file_path = "/tmp/modified_svg_output.svg";
        tokio::fs::write(&temp_file_path, &modified_svg)
            .await
            .expect("Failed to write modified SVG to temporary file");
        eprintln!("Modified SVG written to: {temp_file_path}");

        let modified_svg_str = String::from_utf8_lossy(&modified_svg);
        assert!(modified_svg_str.contains("@font-face{font-family:IosevkaFtl;"));
        assert!(modified_svg_str.contains("src:url(data:font/woff2;base64,"));

        // Remove base64 data URLs before snapshotting
        let modified_svg_str_redacted =
            regex::Regex::new(r"data:font/woff2;base64,[A-Za-z0-9+/=]+")
                .unwrap()
                .replace_all(&modified_svg_str, "data:font/woff2;base64,REDACTED")
                .into_owned();
        insta::assert_snapshot!(modified_svg_str_redacted);

        // Extract and decode base64 font data
        let base64_regex = regex::Regex::new(r"data:font/woff2;base64,([A-Za-z0-9+/=]+)").unwrap();
        for (index, capture) in base64_regex.captures_iter(&modified_svg_str).enumerate() {
            let base64_data = capture.get(1).unwrap().as_str();
            let font_data = base64::engine::general_purpose::STANDARD
                .decode(base64_data)
                .unwrap();

            // Write decoded font to temporary file
            let temp_font_path = format!("/tmp/extracted_font_{index}.woff2");
            tokio::fs::write(&temp_font_path, &font_data).await.unwrap();

            // Use ttx to dump only the 'cmap' table
            let output = tokio::process::Command::new("ttx")
                .arg("-o")
                .arg("-") // Output to stdout
                .arg("-t")
                .arg("cmap") // Only extract the cmap table
                .arg(&temp_font_path)
                .output()
                .await
                .unwrap();

            let cmap_table = String::from_utf8_lossy(&output.stdout);
            insta::assert_snapshot!(format!("cmap_table_{}", index), cmap_table);
        }
    }
}
