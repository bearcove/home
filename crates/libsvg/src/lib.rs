mod drawio_server;

use autotrait::autotrait;
use futures_core::future::BoxFuture;

use bytes::Bytes;
use conflux::{Dimensions, SvgFontFaceCollection};
pub use eyre::Result;

#[derive(Default)]
struct ModImpl;

/// Options when converting a .drawio file to SVG
pub struct DrawioToSvgOptions {
    pub minify: bool,
}

/// Options when cleaning up an SVG
pub struct SvgCleanupOptions {}

#[autotrait]
impl Mod for ModImpl {
    fn drawio_to_svg(
        &self,
        input: Bytes,
        opts: DrawioToSvgOptions,
    ) -> BoxFuture<'_, Result<Vec<u8>>> {
        Box::pin(async move { drawio_server::drawio_to_svg(input, opts).await })
    }

    fn svg_dimensions(&self, input: &[u8]) -> Option<Dimensions> {
        impls::svg_dimensions(input)
    }

    fn inject_font_faces<'future>(
        &'future self,
        input: &'future [u8],
        font_faces: &'future SvgFontFaceCollection,
    ) -> BoxFuture<'future, eyre::Result<Vec<u8>>> {
        Box::pin(async move { impls::inject_font_faces(input, font_faces).await })
    }

    fn cleanup_svg(&self, input: &[u8], opts: SvgCleanupOptions) -> eyre::Result<Vec<u8>> {
        impls::cleanup_svg(input, opts)
    }
}

pub mod char_usage;
pub mod impls;
