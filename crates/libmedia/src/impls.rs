use conflux::MediaKind;
use eyre::eyre;
use html_escape::encode_double_quoted_attribute;
use image_types::ICodec;

use crate::MediaMarkupOpts;

fn is_codec_included_in_markup(ic: ICodec) -> bool {
    match ic {
        ICodec::WEBP | ICodec::AVIF | ICodec::JXL => true,
        ICodec::PNG | ICodec::JPG | ICodec::HEIC => false,
    }
}

/// Generate markup for a media file
pub fn media_markup(opts: MediaMarkupOpts<'_>) -> eyre::Result<String> {
    use std::fmt::Write;
    let tc = &opts.rv.rev()?.ti.tc;
    let mut w = String::new();

    match opts.media.props.kind {
        MediaKind::Audio => {
            // we don't do audio transcoding yet, so the input path is unchanged
            let dest_url = opts.rv.cachebuster().asset_url(opts.web, opts.path)?;

            write!(w, r#"<audio controls"#)?;
            if let Some(id) = opts.id {
                write!(w, r#" id="{}""#, encode_double_quoted_attribute(id))?;
            }
            if let Some(title) = opts.title {
                write!(w, r#" title="{}""#, encode_double_quoted_attribute(title))?;
            }
            if let Some(alt) = opts.alt {
                write!(w, r#" alt="{}""#, encode_double_quoted_attribute(alt))?;
            }
            write!(w, r#">"#)?;
            write!(w, r#"<source src="{dest_url}" type="audio/m4a">"#)?;
            write!(w, "Your browser does not support the audio tag.")?;
            write!(w, "</audio>")?;
        }
        MediaKind::Bitmap => {
            write!(w, "<picture>")?;

            for variant in &opts.media.bv {
                if !is_codec_included_in_markup(variant.ic) {
                    continue;
                }

                let content_type = variant.ic.content_type();

                write!(w, r#"<source type="{content_type}""#)?;
                if let Some(max_width) = variant.max_width {
                    write!(w, r#" media="(max-width: {max_width}px)""#)?;
                }
                write!(w, r#" srcset=""#)?;

                for (i, (density, route)) in variant.srcset.iter().enumerate() {
                    let url = route.to_cdn_url(tc, opts.web);
                    if i >= 1 {
                        write!(w, ",")?;
                    }
                    write!(w, r#"{url} {density}x"#)?;
                }
                write!(w, r#"">"#)?;
            }

            // Find the JXL variant with None width, it goes in the `img` tag
            let jxl_variant = opts
                .media
                .bv
                .iter()
                .find(|v| v.ic == ICodec::JXL && v.max_width.is_none())
                .ok_or_else(|| {
                    eyre!(
                        "No JXL variant with None width available for media: {:?}",
                        opts.media
                    )
                })?;

            let (_preferred_density, preferred_route) = jxl_variant
                .srcset
                .iter()
                .max()
                .expect("JXL variant has no srcset entries");
            let preferred_url = preferred_route.to_cdn_url(tc, opts.web);

            let dims = opts.media.props.dims;
            let logical_img_width = opts
                .width
                .unwrap_or_else(|| dims.w.to_logical(dims.density));
            let logical_img_height = opts
                .height
                .unwrap_or_else(|| dims.h.to_logical(dims.density));
            write!(
                w,
                r#"<img src="{preferred_url}" loading="lazy" width="{logical_img_width}" height="{logical_img_height}" data-kind="media" data-input-path="{}""#,
                encode_double_quoted_attribute(opts.path)
            )?;

            if let Some(id) = opts.id {
                write!(w, r#" id="{}""#, encode_double_quoted_attribute(id))?;
            }
            if let Some(class) = opts.class {
                write!(w, r#" class="{}""#, encode_double_quoted_attribute(class))?;
            }

            if let Some(title) = opts.title {
                write!(w, r#" title="{}""#, encode_double_quoted_attribute(title))?;
            }
            if let Some(alt) = opts.alt {
                write!(w, r#" alt="{}""#, encode_double_quoted_attribute(alt))?;
            }
            w.push_str("></picture>");
        }
        MediaKind::Video => {
            let dims = &opts.media.props.dims;
            let used_width = opts
                .width
                .unwrap_or_else(|| dims.w.to_logical(dims.density));
            let used_height = opts
                .height
                .unwrap_or_else(|| dims.h.to_logical(dims.density));
            write!(
                w,
                r#"<video controls playsinline preload="none" loading="lazy" width="{}" height="{}" data-kind="media" data-input-path="{}""#,
                used_width,
                used_height,
                encode_double_quoted_attribute(opts.path)
            )?;

            if let Some(thumb) = opts.media.thumb.as_ref() {
                let url = thumb.to_cdn_url(tc, opts.web);
                write!(w, r#" poster="{url}""#,)?;
            }

            if let Some(id) = opts.id {
                write!(w, r#" id="{}""#, encode_double_quoted_attribute(id))?;
            }
            if let Some(class) = opts.class {
                write!(w, r#" class="{}""#, encode_double_quoted_attribute(class))?;
            }
            if let Some(title) = opts.title {
                write!(w, r#" title="{}""#, encode_double_quoted_attribute(title))?;
            }
            if let Some(alt) = opts.alt {
                write!(w, r#" alt="{}""#, encode_double_quoted_attribute(alt))?;
            }
            write!(w, r#">"#)?;

            for variant in &opts.media.vv {
                let content_type = variant.qualified_content_type();
                let url = variant.route.to_cdn_url(tc, opts.web);
                write!(w, r#"<source src="{url}" type="{content_type}">"#)?;
            }

            write!(w, "Your browser does not support the video tag.")?;
            write!(w, "</video>")?;
        }
        MediaKind::Diagram => {
            // the asset URL's input path is just the .drawio file, or the .svg one
            let dest_url = opts.rv.cachebuster().asset_url(opts.web, opts.path)?;

            write!(
                w,
                r#"<img src="{dest_url}" loading="lazy" data-kind="diagram" data-input-path="{}""#,
                encode_double_quoted_attribute(opts.path)
            )?;
            let dims = &opts.media.props.dims;
            if let Some(width) = opts.width {
                write!(w, r#" width="{}""#, width)?;
                if let Some(height) = opts.height {
                    write!(w, r#" height="{}""#, height)?;
                } else {
                    write!(w, r#" height="auto""#)?;
                }
            } else if let Some(height) = opts.height {
                write!(w, r#" height="{}""#, height)?;
                write!(w, r#" width="auto""#)?;
            } else {
                let width = dims.w.to_logical(dims.density);
                let height = dims.h.to_logical(dims.density);
                write!(w, r#" width="{}" height="{}""#, width, height)?;
            }

            if let Some(id) = opts.id {
                write!(w, r#" id="{}""#, encode_double_quoted_attribute(id))?;
            }
            if let Some(title) = opts.title {
                write!(w, r#" title="{}""#, encode_double_quoted_attribute(title))?;
            }
            if let Some(alt) = opts.alt {
                write!(w, r#" alt="{}""#, encode_double_quoted_attribute(alt))?;
            }
            w.push('>');
        }
    };

    Ok(w)
}
