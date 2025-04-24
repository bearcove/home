use crate::FormatAnsiStyle;
use anstyle_parse::Perform;
use std::borrow::Cow;

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AnyColor {
    #[default]
    Default,

    Classic(ClassicColor),

    Rgb(u8, u8, u8),

    // 256 colors
    AnsiValue(u8),
}

impl From<ClassicColor> for AnyColor {
    fn from(c: ClassicColor) -> Self {
        Self::Classic(c)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, displaydoc::Display)]
pub(crate) enum ClassicColor {
    /// blk
    Black,
    /// red
    Red,
    /// grn
    Green,
    /// ylw
    Yellow,
    /// blu
    Blue,
    /// mag
    Magenta,
    /// cyn
    Cyan,
    /// wht
    White,

    /// lblk
    LightBlack,
    /// lred
    LightRed,
    /// lgrn
    LightGreen,
    /// lyel
    LightYellow,
    /// lblu
    LightBlue,
    /// lmag
    LightMagenta,
    /// lcyn
    LightCyan,
    /// lwht
    LightWhite,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Weight {
    Faint,
    #[default]
    Normal,
    Bold,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Style {
    pub(crate) fg: AnyColor,
    pub(crate) bg: AnyColor,
    pub(crate) weight: Weight,
    pub(crate) decoration: Decoration,
    pub(crate) font_style: FontStyle,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Decoration {
    Underline,
    Strikethrough,
    #[default]
    None,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FontStyle {
    #[default]
    Normal,
    Italic,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct StyledChar {
    pub(crate) style: Style,
    pub(crate) c: char,
}

impl Default for StyledChar {
    fn default() -> Self {
        Self {
            style: Default::default(),
            c: ' ',
        }
    }
}

#[derive(Default)]
pub(crate) struct Screen {
    pub(crate) lines: Vec<Vec<StyledChar>>,
    pub(crate) style: Style,
    pub(crate) row: usize,
    pub(crate) col: usize,
}

impl Screen {
    pub(crate) fn line_mut(&mut self) -> &mut Vec<StyledChar> {
        while self.lines.len() <= self.row {
            // fill with empty lines
            self.lines.push(Default::default());
        }

        &mut self.lines[self.row]
    }

    pub(crate) fn char_mut(&mut self) -> &mut StyledChar {
        let col = self.col;
        let line = self.line_mut();

        while line.len() <= col {
            // fill with spaces
            line.push(Default::default());
        }

        &mut line[col]
    }

    pub(crate) fn set_color_from<'a>(
        &mut self,
        first: &'a [u16],
        params: &mut impl Iterator<Item = &'a [u16]>,
    ) {
        match first[0] {
            // classic fg colors
            30 => {
                self.style.fg = ClassicColor::Black.into();
            }
            31 => {
                self.style.fg = ClassicColor::Red.into();
            }
            32 => {
                self.style.fg = ClassicColor::Green.into();
            }
            33 => {
                self.style.fg = ClassicColor::Yellow.into();
            }
            34 => {
                self.style.fg = ClassicColor::Blue.into();
            }
            35 => {
                self.style.fg = ClassicColor::Magenta.into();
            }
            36 => {
                self.style.fg = ClassicColor::Cyan.into();
            }
            37 => {
                self.style.fg = ClassicColor::White.into();
            }
            90 => {
                self.style.fg = ClassicColor::LightBlack.into();
            }
            91 => {
                self.style.fg = ClassicColor::LightRed.into();
            }
            92 => {
                self.style.fg = ClassicColor::LightGreen.into();
            }
            93 => {
                self.style.fg = ClassicColor::LightYellow.into();
            }
            94 => {
                self.style.fg = ClassicColor::LightBlue.into();
            }
            95 => {
                self.style.fg = ClassicColor::LightMagenta.into();
            }
            96 => {
                self.style.fg = ClassicColor::LightCyan.into();
            }
            97 => {
                self.style.fg = ClassicColor::LightWhite.into();
            }

            // classic bg colors
            40 => {
                self.style.bg = ClassicColor::Black.into();
            }
            41 => {
                self.style.bg = ClassicColor::Red.into();
            }
            42 => {
                self.style.bg = ClassicColor::Green.into();
            }
            43 => {
                self.style.bg = ClassicColor::Yellow.into();
            }
            44 => {
                self.style.bg = ClassicColor::Blue.into();
            }
            45 => {
                self.style.bg = ClassicColor::Magenta.into();
            }
            46 => {
                self.style.bg = ClassicColor::Cyan.into();
            }
            47 => {
                self.style.bg = ClassicColor::White.into();
            }
            100 => {
                self.style.bg = ClassicColor::LightBlack.into();
            }
            101 => {
                self.style.bg = ClassicColor::LightRed.into();
            }
            102 => {
                self.style.bg = ClassicColor::LightGreen.into();
            }
            103 => {
                self.style.bg = ClassicColor::LightYellow.into();
            }
            104 => {
                self.style.bg = ClassicColor::LightBlue.into();
            }
            105 => {
                self.style.bg = ClassicColor::LightMagenta.into();
            }
            106 => {
                self.style.bg = ClassicColor::LightCyan.into();
            }
            107 => {
                self.style.bg = ClassicColor::LightWhite.into();
            }

            // 256 colors and 24-bit color
            38 | 48 => {
                let is_fg = first[0] == 38;
                let kind = params.next().unwrap();
                if kind[0] == 5 {
                    let color = params.next().unwrap()[0] as u8;
                    if is_fg {
                        self.style.fg = AnyColor::AnsiValue(color);
                    } else {
                        self.style.bg = AnyColor::AnsiValue(color);
                    }
                } else if kind[0] == 2 {
                    let r = params.next().unwrap()[0] as u8;
                    let g = params.next().unwrap()[0] as u8;
                    let b = params.next().unwrap()[0] as u8;
                    if is_fg {
                        self.style.fg = AnyColor::Rgb(r, g, b);
                    } else {
                        self.style.bg = AnyColor::Rgb(r, g, b);
                    }
                } else {
                    panic!(
                        "unrecognized {}-color: {:?}",
                        if is_fg { 38 } else { 48 },
                        kind
                    );
                }
            }

            c => {
                panic!("unrecognized color: {:?}", c);
            }
        }
    }
}

macro_rules! declare_usize_backed_enum {
    ($name:ident { $($variant:ident = $value:expr,)* }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub(crate) enum $name {
            $($variant = $value,)*
        }

        impl TryFrom<usize> for $name {
            type Error = ();
            fn try_from(value: usize) -> Result<Self, Self::Error> {
                match value {
                    $($value => Ok($name::$variant),)*
                    _ => Err(()),
                }
            }
        }
    };
}

declare_usize_backed_enum!(CsiLhMode {
    Unknown1 = 1,
    ShowCursor = 25,
    EnableMouseTracking = 1000,
    EnableMouseTrackingWithDragEvents = 1002,
    EnableMouseTrackingWithAllMotion = 1003,
    ReportFocus = 1004,
    EnableSgrExtendedMouseCoordinates = 1006,
    EnableUrxvtExtendedMouseCoordinates = 1015,
    EnableAlternateScreenBuffer = 1049,
    EnableBracketedPasteMode = 2004,
});

#[derive(Default)]
pub(crate) struct Performer {
    pub(crate) screen: Screen,
    pub(crate) alt_screen: Screen,
    pub(crate) strict: bool,
}

impl Perform for Performer {
    fn print(&mut self, c: char) {
        tracing::trace!(
            "print: {} ({}) - style: {:?}\r",
            c,
            c as u64,
            self.screen.style
        );
        {
            let style = self.screen.style;
            let cm = self.screen.char_mut();
            cm.c = c;
            cm.style = style;
        }
        self.screen.col += 1;
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => {
                // bell, ignore
            }
            0x08 => {
                // backspace
                if self.screen.col > 0 {
                    self.screen.col -= 1;
                }
            }
            0x09 => {
                // tab, move to next multiple of 8
                let col = self.screen.col;
                let next = col + (8 - (col % 8));
                self.screen.col = next;
            }
            0x0A => {
                // line feed, this creates a new line.
                self.screen.row += 1;
            }
            0x0C => {
                // form feed, ignore
            }
            0x0D => {
                // carriage return
                self.screen.col = 0;
            }
            x => {
                if self.strict {
                    panic!("unrecognized execute character: \\x{:02x}", x);
                }
            }
        }
    }

    fn hook(
        &mut self,
        _params: &anstyle_parse::Params,
        _intermediates: &[u8],
        _ignore: bool,
        _action: u8,
    ) {
        if self.strict {
            panic!("unsupported: hook: {}", _action as char);
        } else {
            tracing::warn!("unsupported: hook: {}", _action as char);
        }
    }

    fn put(&mut self, _byte: u8) {
        if self.strict {
            panic!("unsupported: put: {}", _byte);
        } else {
            tracing::warn!("unsupported: put: {}", _byte);
        }
    }

    fn unhook(&mut self) {
        if self.strict {
            panic!("unsupported: unhook");
        } else {
            tracing::warn!("unsupported: unhook");
        }
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {
        tracing::trace!(
            "osc_dispatch: params={:?}, bell_terminated={}",
            _params,
            _bell_terminated
        );
        // OSC can be used for various things, like setting window title,
        // clipboard content, color palette, etc. — we don't really need
        // it right now, so let's just ignore it.
    }

    fn csi_dispatch(
        &mut self,
        params: &anstyle_parse::Params,
        intermediates: &[u8],
        ignore: bool,
        action: u8,
    ) {
        if ignore {
            return;
        }

        tracing::trace!(
            "csi_dispatch: {:?} {:?} {} {:?}\r",
            params,
            intermediates,
            ignore,
            action as char
        );

        match action {
            b'm' => {
                let mut params = params.iter();
                while let Some(param1) = params.next() {
                    match param1[0] {
                        0 => {
                            // reset
                            self.screen.style = Default::default();
                        }
                        1 => {
                            // bold
                            self.screen.style.weight = Weight::Bold;
                        }
                        2 => {
                            // faint
                            self.screen.style.weight = Weight::Faint;
                        }
                        3 => {
                            // italic
                            self.screen.style.font_style = FontStyle::Italic;
                        }
                        4 => {
                            // underline
                            self.screen.style.decoration = Decoration::Underline;
                        }
                        5 => {
                            // slow blink, ignore
                        }
                        6 => {
                            // rapid blink, ignore
                        }
                        7 => {
                            // reverse video?
                            // who would use this??? - amos
                            std::mem::swap(&mut self.screen.style.fg, &mut self.screen.style.bg);
                        }
                        8 => {
                            // conceal, ignore
                        }
                        9 => {
                            // crossed out
                            self.screen.style.decoration = Decoration::Strikethrough;
                        }
                        10 => {
                            // primary font, ignore
                        }
                        11..=19 => {
                            // alternate font, ignore
                        }
                        20 => {
                            // fraktur, ignore
                        }
                        21 => {
                            // bold off or double underline, ignore
                        }
                        22 => {
                            // normal weight
                            self.screen.style.weight = Weight::Normal;
                        }
                        23 => {
                            // not italic
                            self.screen.style.font_style = FontStyle::Normal;
                        }
                        24 => {
                            // not underlined
                            self.screen.style.decoration = Decoration::None;
                        }
                        25 => {
                            // not blinking, ignore
                        }
                        26 => {
                            // proportional spacing, ignore
                        }
                        27 => {
                            // not reverse video
                            std::mem::swap(&mut self.screen.style.fg, &mut self.screen.style.bg);
                        }
                        28 => {
                            // reveal, ignore
                        }
                        29 => {
                            // not crossed out
                            self.screen.style.decoration = Decoration::None;
                        }
                        30..=37 => {
                            // classic fg color range, forward to set_color_from
                            self.screen.set_color_from(param1, &mut params);
                        }
                        38 => {
                            // fg color
                            self.screen.set_color_from(param1, &mut params);
                        }
                        39 => {
                            // default fg color
                            self.screen.style.fg = AnyColor::Default;
                        }
                        40..=47 => {
                            // classic bg color range, forward to set_color_from
                            self.screen.set_color_from(param1, &mut params);
                        }
                        48 => {
                            // bg color
                            self.screen.set_color_from(param1, &mut params);
                        }
                        49 => {
                            // default bg color
                            self.screen.style.bg = AnyColor::Default;
                        }
                        50 => {
                            // disable proportional spacing, ignore
                        }
                        51 => {
                            // framed, ignore
                        }
                        52 => {
                            // encircled, ignore
                        }
                        53 => {
                            // overlined, ignore
                        }
                        54 => {
                            // not framed or encircled, ignore
                        }
                        55 => {
                            // not overlined, ignore
                        }
                        58 => {
                            // underline color, ignore
                        }
                        59 => {
                            // default underline color, ignore
                        }
                        60..=65 => {
                            // ideogram underline or right side line, ignore
                        }
                        73..=75 => {
                            // superscript & subscript, ignore
                        }
                        90..=97 => {
                            // classic fg color range, forward to set_color_from
                            self.screen.set_color_from(param1, &mut params);
                        }
                        100..=107 => {
                            // classic bg color range, forward to set_color_from
                            self.screen.set_color_from(param1, &mut params);
                        }
                        _ => {
                            if self.strict {
                                panic!("unimplemented SGR: {:?}", param1);
                            }
                        }
                    }
                }

                // colors!
            }
            b'h' => {
                let param1 = params.iter().next().unwrap_or(&[1])[0] as usize;
                match CsiLhMode::try_from(param1) {
                    Ok(CsiLhMode::EnableAlternateScreenBuffer) => {
                        std::mem::swap(&mut self.screen, &mut self.alt_screen);
                    }
                    Ok(_) => {
                        // no side-effect for these as far as we're concerned
                    }
                    Err(()) => {
                        if self.strict {
                            panic!("unrecognized 'h' mode: {param1}");
                        }
                    }
                }
            }
            b'l' => {
                let param1 = params.iter().next().unwrap_or(&[1])[0] as usize;
                match CsiLhMode::try_from(param1) {
                    Ok(CsiLhMode::EnableAlternateScreenBuffer) => {
                        std::mem::swap(&mut self.screen, &mut self.alt_screen);
                    }
                    Ok(_) => {
                        // no side-effect for these as far as we're concerned
                    }
                    Err(()) => {
                        if self.strict {
                            panic!("unrecognized 'l' mode: {param1}");
                        }
                    }
                }
            }
            b'A' => {
                // move up
                let param1 = params.iter().next().unwrap_or(&[1])[0].max(1) as usize;

                self.screen.row = self.screen.row.saturating_sub(param1);
            }
            b'B' => {
                // move down
                let param1 = params.iter().next().unwrap_or(&[1])[0].max(1) as usize;
                self.screen.row += param1;
            }
            b'C' => {
                // move right
                let param1 = params.iter().next().unwrap_or(&[1])[0].max(1) as usize;
                self.screen.col += param1;
            }
            b'D' => {
                // move left
                let param1 = params.iter().next().unwrap_or(&[1])[0].max(1) as usize;
                self.screen.col = self.screen.col.saturating_sub(param1);
            }
            b'E' => {
                // move down and to column 1
                let param1 = params.iter().next().unwrap_or(&[1])[0] as usize;
                self.screen.row += param1;
                self.screen.col = 0;
            }
            b'F' => {
                // move up and to column 1
                let param1 = params.iter().next().unwrap_or(&[1])[0] as usize;
                self.screen.row = self.screen.row.saturating_sub(param1);
                self.screen.col = 0;
            }
            b'J' => {
                // erase in display
                let param1 = params.iter().next().unwrap_or(&[0]);

                match param1[0] {
                    0 => {
                        // clear from cursor to end of screen
                        let col = self.screen.col;

                        for row in self.screen.row..self.screen.lines.len() {
                            if row >= self.screen.lines.len() {
                                continue;
                            }

                            let line = &mut self.screen.lines[row];
                            if row == self.screen.row {
                                (col..line.len()).for_each(|i| {
                                    line[i].c = ' ';
                                });
                            } else {
                                for sc in line {
                                    sc.c = ' ';
                                }
                            }
                        }
                    }
                    1 => {
                        // clear from start of screen to cursor
                        let col = self.screen.col;

                        for row in 0..self.screen.row {
                            if row >= self.screen.lines.len() {
                                continue;
                            }

                            let line = &mut self.screen.lines[row];
                            if row == self.screen.row {
                                (0..=col).for_each(|i| {
                                    line[i].c = ' ';
                                });
                            } else {
                                for sc in line {
                                    sc.c = ' ';
                                }
                            }
                        }
                    }
                    2..=3 => {
                        // erase entire screen
                        for line in &mut self.screen.lines {
                            for sc in line {
                                sc.c = ' ';
                            }
                        }
                    }
                    _ => {
                        panic!("unrecognized erase in display: {:?}", param1);
                    }
                }
            }
            b'K' => {
                // erase in line
                let param1 = params.iter().next().unwrap_or(&[0]);

                match param1[0] {
                    0 => {
                        // erase from cursor to end of line
                        let col = self.screen.col;
                        let line = self.screen.line_mut();
                        (col..line.len()).for_each(|i| {
                            line[i].c = ' ';
                        });
                    }
                    1 => {
                        // erase from start of line to cursor
                        let col = self.screen.col;
                        let line = self.screen.line_mut();
                        (0..=col).for_each(|i| {
                            line[i].c = ' ';
                        });
                    }
                    2 => {
                        // erase entire line
                        let line = self.screen.line_mut();
                        for sc in line {
                            sc.c = ' ';
                        }
                    }
                    _ => {
                        panic!("unrecognized erase in line: {:?}", param1);
                    }
                }
            }
            b'H' => {
                // cursor position
                let mut params = params.iter();
                let row = params.next().unwrap_or(&[1])[0] as usize;
                let col = params.next().unwrap_or(&[1])[0] as usize;

                self.screen.row = row - 1;
                self.screen.col = col - 1;
            }
            b'G' => {
                // cursor horizontal absolute
                let param1 = params.iter().next().unwrap_or(&[1])[0] as usize;
                self.screen.col = param1 - 1;
            }
            b'P' => {
                // delete character
                let param1 = params.iter().next().unwrap_or(&[1])[0].max(1) as usize;
                let col = self.screen.col;
                let line = self.screen.line_mut();

                if col < line.len() {
                    // shift everything left by param1 positions
                    line.copy_within(col + param1.., col);

                    // fill the end with spaces
                    for i in (line.len() - param1)..line.len() {
                        line[i] = Default::default();
                    }
                }
            }
            b'u' => {
                // "save cursor" / "restore cursor"? unclear even after reading <https://www.xfree86.org/current/ctlseqs.pdf>
                // and other sources, especially that I'm seeing it in the wild with params 13 62:
                // unrecognized csi_dispatch: [13] [62] false 'u'
            }
            _ => {
                if self.strict {
                    panic!(
                        "unrecognized csi_dispatch: {:?} {:?} {} {:?}\r",
                        params, intermediates, ignore, action as char
                    )
                }
            }
        }
    }

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {
        tracing::debug!("esc_dispatch: {:?} {:?}", _intermediates, _byte as char);
    }
}

impl Performer {
    pub(crate) fn finish(self, ansi_style: FormatAnsiStyle) -> String {
        // look for lines containing PROMPT_CHAR, and remove two lines before it
        const PROMPT_CHAR: char = '❯';

        let mut lines = self.screen.lines;
        if let Some((i, _)) = lines
            .iter()
            .enumerate()
            .rev()
            .find(|(_, line)| line.iter().any(|sc| sc.c == PROMPT_CHAR))
        {
            lines.truncate(i - 2);
        }

        // collect chars from self.line into a new String,
        // but ignore any trailing whitespace
        let mut output = String::new();

        if ansi_style == FormatAnsiStyle::Markdown {
            output.push_str("```term\n");
        }

        let mut has_encountered_non_whitespace = false;

        let mut style: Option<Style> = None;
        for line in lines {
            let line_is_all_whitespace = line.iter().all(|sc| sc.c.is_whitespace());
            if line_is_all_whitespace {
                if has_encountered_non_whitespace {
                    // only whitespace: just add a newline
                    output.push('\n');
                    continue;
                } else {
                    // only whitespace: skip
                    continue;
                }
            } else {
                has_encountered_non_whitespace = true;
            }

            for c in line {
                // if LF, trim all trailing whitespace
                if c.c == '\n' {
                    output.truncate(output.trim_end_matches(' ').len());
                }

                if style.unwrap_or_default() != c.style {
                    if style.is_some() {
                        output.push_str("</i>");
                    }

                    if c.style == Default::default() {
                        style = None;
                    } else {
                        output.push_str("<i");

                        let mut style_directives: Vec<String> = vec![];

                        let mut classes: Vec<Cow<str>> = vec![];
                        match c.style.weight {
                            Weight::Faint => {
                                classes.push("l".into());
                            }
                            Weight::Normal => {}
                            Weight::Bold => {
                                classes.push("b".into());
                            }
                        }

                        match c.style.decoration {
                            Decoration::Underline => {
                                classes.push("u".into());
                            }
                            Decoration::Strikethrough => {
                                classes.push("st".into());
                            }
                            Decoration::None => {
                                // nothing
                            }
                        }

                        match c.style.font_style {
                            FontStyle::Normal => {
                                // nothing
                            }
                            FontStyle::Italic => {
                                classes.push("i".into());
                            }
                        }

                        match c.style.fg {
                            AnyColor::Default => {
                                // nothing
                            }
                            AnyColor::Classic(c) => {
                                classes.push(format!("fg-{}", c).into());
                            }
                            AnyColor::Rgb(r, g, b) => {
                                style_directives
                                    .push(format!("color:#{:02x}{:02x}{:02x}", r, g, b));
                            }
                            AnyColor::AnsiValue(c) => {
                                classes.push(format!("fg-ansi{c}").into());
                            }
                        }

                        match c.style.bg {
                            AnyColor::Default => {
                                // nothing
                            }
                            AnyColor::Classic(c) => {
                                classes.push(format!("bg-{}", c).into());
                            }
                            AnyColor::Rgb(r, g, b) => {
                                style_directives
                                    .push(format!("background:#{:02x}{:02x}{:02x}", r, g, b));
                            }
                            AnyColor::AnsiValue(c) => {
                                classes.push(format!("bg-ansi{c}").into());
                            }
                        }

                        // format classes
                        if !classes.is_empty() {
                            output.push_str(" class=\"");
                            output.push_str(&classes.join(" "));
                            output.push('"');
                        }

                        // format style
                        if !style_directives.is_empty() {
                            output.push_str(" style=\"");
                            output.push_str(&style_directives.join(";"));
                            output.push('"');
                        }

                        output.push('>');
                        style = Some(c.style);
                    }
                }

                // html entities for a few things
                match c.c {
                    '&' => output.push_str("&amp;"),
                    '<' => output.push_str("&lt;"),
                    '>' => output.push_str("&gt;"),
                    '`' => output.push_str("&#96;"),
                    _ => output.push(c.c),
                }
            }

            // truncate all the spaces at the end of the line
            output.truncate(output.trim_end_matches(' ').len());

            // close style if open
            if style.is_some() {
                output.push_str("</i>");
                style = None;
            }

            output.push('\n');
        }

        // trim all whitespace
        output.truncate(output.trim_end_matches(' ').len());

        if style.is_some() {
            output.push_str("</i>");
        }
        if ansi_style == FormatAnsiStyle::Markdown {
            output.push_str("```\n");
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use crate::{FormatAnsiStyle, Mod, ModImpl};

    #[test]
    fn test_ansi_formatting_html() {
        let mod_instance = ModImpl;

        let input = "\x1b[1;31mBold Red\x1b[0m\n\x1b[4;32mUnderline Green\x1b[0m\n\x1b[3;33mItalic Yellow\x1b[0m";
        let result = mod_instance.format_ansi(input, FormatAnsiStyle::Markdown);

        let expected = "```term\n<i class=\"b fg-red\">Bold Red</i>\n<i class=\"u fg-grn\">Underline Green</i>\n<i class=\"i fg-ylw\">Italic Yellow</i>\n```\n";
        assert_eq!(result, expected);
    }
}
