use conflux::Route;
use merde::{DeserOpinions, time::Rfc3339};
use time::OffsetDateTime;

#[derive(Debug)]
pub struct Frontmatter {
    /// Title of the page
    pub title: String,

    /// Jinja2 template to use for rendering — defaults to `page.html`
    pub template: String,

    /// Publication date in RFC3339 format, e.g. `2023-10-01T12:00:00Z` (UTC)
    pub date: Rfc3339<OffsetDateTime>,

    /// Last update date, if any
    pub updated_at: Option<Rfc3339<OffsetDateTime>>,

    /// If true, page is only visible by admins
    pub draft: bool,

    /// Whether the page should be excluded from search indexing
    pub archive: bool,

    /// Code used to allow access to a draft
    pub draft_code: Option<String>,

    /// Alternative routes for this page (for redirects)
    pub aliases: Vec<Route>,

    /// Tags associated with the page (useful for listings)
    pub tags: Vec<String>,

    /// Additional metadata for the page
    pub extra: FrontmatterExtras,
}

#[derive(Default, Debug)]
pub struct FrontmatterExtras {
    // show patreon credits
    pub patreon: bool,

    // don't show reddit comments button
    pub hide_comments: bool,

    // don't show patreon plug
    pub hide_patreon: bool,

    // don't show date, author, etc.
    pub hide_metadata: bool,

    // tube slug
    pub tube: Option<String>,

    // youtube video ID
    pub youtube: Option<String>,

    // whether this is a dual feature (show the video while the article is still exclusive)
    pub dual_feature: bool,

    // video duration
    pub duration: Option<u64>,

    // for a series, marks whether it's still ongoing
    pub ongoing: bool,
}

pub struct FrontmatterIn {
    /// Title of the page
    pub title: String,

    /// Jinja2 template to use for rendering — defaults to `page.html`
    pub template: Option<String>,

    /// Publication date in RFC3339 format, e.g. `2023-10-01T12:00:00Z` (UTC)
    pub date: Rfc3339<OffsetDateTime>,

    /// Last update date, if any
    pub updated_at: Option<Rfc3339<OffsetDateTime>>,

    /// If true, page is only visible by admins
    pub draft: Option<bool>,

    /// Whether the page should be excluded from search indexing
    pub archive: Option<bool>,

    /// Code used to allow access to a draft
    pub draft_code: Option<String>,

    /// Alternative routes for this page (for redirects)
    pub aliases: Option<Vec<Route>>,

    /// Tags associated with the page (useful for listings)
    pub tags: Option<Vec<String>>,

    /// Additional metadata for the page
    pub extra: Option<FrontmatterExtrasIn>,
}

// TODO: implement defaults via merde's default mechanism

struct FrontMatterInOpinions;

impl DeserOpinions for FrontMatterInOpinions {
    fn deny_unknown_fields(&self) -> bool {
        false
    }

    fn map_key_name<'s>(&self, key: merde::CowStr<'s>) -> merde::CowStr<'s> {
        if key == "draft-code" {
            "draft_code".into()
        } else {
            key
        }
    }

    fn default_field_value<'borrow>(
        &self,
        _key: &'borrow str,
        _slot: merde::FieldSlot<'_, 'borrow>,
    ) {
        // don't fill in
    }
}

merde::derive! {
    impl (Deserialize) for struct FrontmatterIn {
        title,
        template,
        date,
        updated_at,
        draft,
        archive,
        draft_code,
        aliases,
        tags,
        extra
    } via FrontMatterInOpinions
}

impl From<FrontmatterIn> for Frontmatter {
    fn from(frontmatter_in: FrontmatterIn) -> Self {
        Self {
            title: frontmatter_in.title,
            template: frontmatter_in.template.unwrap_or("page.html".into()),
            date: frontmatter_in.date.0.into(),
            updated_at: frontmatter_in.updated_at.map(|d| d.0.into()),
            draft: frontmatter_in.draft.unwrap_or_default(),
            archive: frontmatter_in.archive.unwrap_or_default(),
            draft_code: frontmatter_in.draft_code,
            aliases: frontmatter_in.aliases.unwrap_or_default(),
            tags: frontmatter_in.tags.unwrap_or_default(),
            extra: frontmatter_in.extra.unwrap_or_default().into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FrontmatterExtrasIn {
    pub patreon: Option<bool>,
    pub hide_comments: Option<bool>,
    pub hide_patreon: Option<bool>,
    pub hide_metadata: Option<bool>,
    pub tube: Option<String>,
    pub dual_feature: Option<bool>,
    pub youtube: Option<String>,
    pub duration: Option<u64>,
    pub ongoing: Option<bool>,
}

merde::derive! {
    impl (Deserialize) for struct FrontmatterExtrasIn {
        patreon,
        hide_comments,
        hide_patreon,
        hide_metadata,
        tube,
        dual_feature,
        youtube,
        duration,
        ongoing
    }
}

impl From<FrontmatterExtrasIn> for FrontmatterExtras {
    fn from(frontmatter_extras_in: FrontmatterExtrasIn) -> Self {
        Self {
            patreon: frontmatter_extras_in.patreon.unwrap_or_default(),
            hide_comments: frontmatter_extras_in.hide_comments.unwrap_or_default(),
            hide_patreon: frontmatter_extras_in.hide_patreon.unwrap_or_default(),
            hide_metadata: frontmatter_extras_in.hide_metadata.unwrap_or_default(),
            tube: frontmatter_extras_in.tube,
            dual_feature: frontmatter_extras_in.dual_feature.unwrap_or_default(),
            youtube: frontmatter_extras_in.youtube,
            duration: frontmatter_extras_in.duration,
            ongoing: frontmatter_extras_in.ongoing.unwrap_or_default(),
        }
    }
}
