use autotrait::autotrait;
use eyre::Context;
use image_types::{ICodec, IntrinsicPixels};
use std::{io::Write, time::Instant};

use image::{DynamicImage, ImageDecoder, Rgb, Rgba};
use jpegxl_rs::encode::EncoderFrame;
use jxl_oxide::JxlImage;
use rgb::FromSlice;

struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

pub use eyre::Result;

#[autotrait]
impl Mod for ModImpl {
    fn transcode(
        &self,
        input: &[u8],
        ifmt: ICodec,
        ofmt: ICodec,
        target_width: Option<IntrinsicPixels>,
    ) -> Result<Vec<u8>> {
        let start_load = Instant::now();

        // Load the image from the input bytes
        let mut img = match ifmt {
            ICodec::PNG => image::load_from_memory_with_format(input, image::ImageFormat::Png)?,
            ICodec::JPG => image::load_from_memory_with_format(input, image::ImageFormat::Jpeg)?,
            ICodec::WEBP => image::load_from_memory_with_format(input, image::ImageFormat::WebP)?,
            ICodec::AVIF => image::load_from_memory_with_format(input, image::ImageFormat::Avif)?,
            ICodec::JXL => {
                let image = JxlImage::builder()
                    .read(input)
                    .map_err(|e| eyre::eyre!("jxl decoding error: {e}"))?;
                let fb = image
                    .render_frame(0)
                    .map_err(|e| eyre::eyre!("jxl rendering error: {e}"))
                    .wrap_err("jxl rendering error")?
                    .image();
                match fb.channels() {
                    3 => DynamicImage::from(
                        image::ImageBuffer::<Rgb<f32>, Vec<f32>>::from_raw(
                            fb.width() as u32,
                            fb.height() as u32,
                            fb.buf().to_vec(),
                        )
                        .ok_or_else(|| {
                            eyre::eyre!("failed to create ImageBuffer from jxl frame (RGB)")
                        })?,
                    ),
                    4 => DynamicImage::from(
                        image::ImageBuffer::<Rgba<f32>, Vec<f32>>::from_raw(
                            fb.width() as u32,
                            fb.height() as u32,
                            fb.buf().to_vec(),
                        )
                        .ok_or_else(|| {
                            eyre::eyre!("failed to create ImageBuffer from jxl frame (RGBA)")
                        })?,
                    ),
                    _ => {
                        unimplemented!(
                            "unsupported number of channels in jxl image: {}",
                            fb.channels()
                        )
                    }
                }
            }
            ICodec::HEIC => {
                let mut temp_heic = tempfile::NamedTempFile::new()
                    .wrap_err("failed to create temporary file for HEIC input")?;
                temp_heic
                    .write_all(input)
                    .wrap_err("failed to write HEIC data to temporary file")?;

                let temp_png = tempfile::NamedTempFile::new()
                    .wrap_err("failed to create temporary file for HEIC output")?;
                let temp_png_path = temp_png
                    .path()
                    .to_str()
                    .ok_or_else(|| eyre::eyre!("failed to get temporary png path as string"))?;

                let status = std::process::Command::new("magick")
                    .arg(temp_heic.path())
                    .arg(format!("png:{temp_png_path}"))
                    .status()
                    .wrap_err("failed to run imagemagick convert command")?;

                if !status.success() {
                    return Err(eyre::eyre!(
                        "imagemagick convert failed with status: {}",
                        status
                    ));
                }

                let png_data = fs_err::read(temp_png_path)
                    .wrap_err("failed to read temporary PNG output file")?;

                image::load_from_memory_with_format(png_data.as_slice(), image::ImageFormat::Png)
                    .wrap_err("failed to load temporary PNG output into image")?
            }
        };

        let duration_load = start_load.elapsed();

        let start_resize = Instant::now();
        if let Some(target_width) = target_width {
            // The image::imageops::resize() function preserves aspect ratio while scaling
            // to the maximum size that fits within the given width/height bounds. So we
            // can just pass the original image height - the output will maintain aspect
            // ratio while ensuring width == target_width.
            img = img.resize(
                target_width.into_inner(),
                img.height(),
                image::imageops::FilterType::Lanczos3,
            );
        }
        let duration_resize = start_resize.elapsed();

        let start_transcode = Instant::now();

        // Encode the image into the output format
        let vec = match ofmt {
            ICodec::AVIF => {
                let encoder = ravif::Encoder::new()
                    .with_quality(85.0)
                    .with_alpha_quality(85.0)
                    .with_num_threads(Some(num_cpus::get()))
                    .with_speed(4); // 3 is _really slow_ (15 seconds on brat!)

                let res = if img.color().has_alpha() {
                    // not as, because it might be a non-u8 subpixel format
                    let rgba = img.to_rgba8();
                    let img = ravif::Img::new(
                        rgba.as_raw().as_rgba(),
                        img.width() as _,
                        img.height() as _,
                    );
                    encoder.encode_rgba(img).wrap_err("ravif_error")?
                } else {
                    let rgb = img.to_rgb8();
                    let img =
                        ravif::Img::new(rgb.as_raw().as_rgb(), img.width() as _, img.height() as _);
                    encoder.encode_rgb(img).wrap_err("ravif error")?
                };

                res.avif_file
            }
            ICodec::WEBP => {
                // the WebP encoder only supports RGBA
                let img = img.to_rgba8();
                let img = DynamicImage::from(img);
                webp::Encoder::from_image(&img)
                    .map_err(|e| eyre::eyre!("webp encoder error: {e}"))
                    .wrap_err("webp error")?
                    .encode(82.0)
                    .to_vec()
            }
            ICodec::PNG => {
                use image::ImageEncoder as _;
                let mut bytes: Vec<u8> = Vec::new();
                let img = img.to_rgba8(); // sometimes we get Rgb32F (out of a JPEG-XL) and PNG does _not_ like that.
                let img = DynamicImage::from(img);
                image::codecs::png::PngEncoder::new(&mut bytes)
                    .write_image(
                        img.as_bytes(),
                        img.width(),
                        img.height(),
                        img.color().into(),
                    )
                    .wrap_err("png encoding error")?;
                bytes
            }
            ICodec::JXL => {
                let runner = jpegxl_rs::ThreadsRunner::default();

                let mut encoder = jpegxl_rs::encoder_builder()
                    .parallel_runner(&runner)
                    .quality(2.8) // that's distance, actually (lower is better)
                    .speed(jpegxl_rs::encode::EncoderSpeed::Squirrel) // effort, 7
                    .build()
                    .wrap_err("jpegxl encoder build error")?; // Replaced bs() with wrap_err

                // Handle RGB and RGBA cases separately
                if img.color().has_alpha() {
                    let rgba = img.to_rgba8();
                    encoder.has_alpha = true;
                    let frame = EncoderFrame::new(rgba.as_raw()).num_channels(4);
                    encoder
                        .encode_frame::<_, u8>(&frame, img.width(), img.height())
                        .wrap_err("jpegxl rgba frame encoding error")? // Replaced bs() with wrap_err
                        .data
                } else {
                    let rgb = img.to_rgb8();
                    encoder.has_alpha = false;
                    let frame = EncoderFrame::new(rgb.as_raw()).num_channels(3);
                    encoder
                        .encode_frame::<_, u8>(&frame, img.width(), img.height())
                        .wrap_err("jpegxl rgb frame encoding error")? // Replaced bs() with wrap_err
                        .data
                }
            }
            _ => {
                return Err(eyre::eyre!(
                    // Replaced BS::from_string with eyre::eyre!
                    "unsupported image format: {:?}",
                    ofmt
                ));
            }
        };

        let duration_transcode = start_transcode.elapsed();
        tracing::info!(
            "\x1b[36m{:?}\x1b[0m => \x1b[36m{:?}\x1b[0m, load: \x1b[33m{:?}\x1b[0m, resize: \x1b[33m{:?}\x1b[0m, transcode: \x1b[33m{:?}\x1b[0m, total: \x1b[33m{:?}\x1b[0m",
            ifmt,
            ofmt,
            duration_load,
            duration_resize,
            duration_transcode,
            duration_load + duration_resize + duration_transcode
        );

        Ok(vec)
    }

    fn dimensions(&self, input: &[u8], ifmt: ICodec) -> Result<(IntrinsicPixels, IntrinsicPixels)> {
        let input = std::io::Cursor::new(input);

        let (width, height) = match ifmt {
            ICodec::PNG => {
                let decoder = image::codecs::png::PngDecoder::new(input)
                    .wrap_err("failed to create PNG decoder")?;
                decoder.dimensions()
            }
            ICodec::JPG => {
                let decoder = image::codecs::jpeg::JpegDecoder::new(input)
                    .wrap_err("failed to create JPG decoder")?;
                decoder.dimensions()
            }
            ICodec::WEBP => {
                let decoder = image::codecs::webp::WebPDecoder::new(input)
                    .wrap_err("failed to create WEBP decoder")?;
                decoder.dimensions()
            }
            ICodec::AVIF => {
                let decoder = image::codecs::avif::AvifDecoder::new(input)
                    .wrap_err("failed to create AVIF decoder")?;
                decoder.dimensions()
            }
            ICodec::JXL => {
                let image = JxlImage::builder()
                    .read(input)
                    .map_err(|e| eyre::eyre!("jxl decoding error: {e}"))?;
                (image.width(), image.height())
            }
            ICodec::HEIC => {
                // Using ImageMagick is probably too slow/heavy just for dimensions.
                // A dedicated HEIC dimensions reader would be better, but for now,
                // mark as unsupported.
                return Err(eyre::eyre!("heic dimensions: unsupported :("));
            }
        };
        Ok((IntrinsicPixels::from(width), IntrinsicPixels::from(height)))
    }
}
