use conflux::{Champion, Route};
use facet::Facet;
use time::OffsetDateTime;

#[derive(Facet, Debug)]
pub struct Frontmatter {
    /// Title of the page
    pub title: String,

    /// Jinja2 template to use for rendering — defaults to `page.html`
    #[facet(default = "page.html".into())]
    pub template: String,

    /// Publication date in RFC3339 format, e.g. `2023-10-01T12:00:00Z` (UTC)
    pub date: OffsetDateTime,

    /// Date at which patrons/sponsors get early access to the page
    #[facet(default)]
    pub early_access_date: Option<OffsetDateTime>,

    /// Last update date, if any
    #[facet(default)]
    pub updated_at: Option<OffsetDateTime>,

    /// If true, page is only visible by admins
    #[facet(default = false)]
    pub draft: bool,

    /// Whether the page should be excluded from search indexing
    #[facet(default = false)]
    pub archive: bool,

    /// Code used to allow access to a draft
    #[facet(rename = "draft-code", default)]
    pub draft_code: Option<String>,

    /// Alternative routes for this page (for redirects)
    #[facet(default)]
    pub aliases: Vec<Route>,

    /// Tags associated with the page (useful for listings)
    #[facet(default)]
    pub tags: Vec<String>,

    /// Additional metadata for the page
    #[facet(default)]
    pub extra: FrontmatterExtras,
}

#[derive(Facet, Default, Debug)]
#[facet(default)]
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

    // bunnystream video ID (used by video template)
    pub bunnystream: Option<String>,

    // whether this is a dual feature (show the video while the article is still exclusive)
    pub dual_feature: bool,

    // the champion of this page (offered an exclusive to the community)
    pub champion: Option<Champion>,

    // video duration
    pub duration: Option<u64>,

    // for a series, marks whether it's still ongoing
    pub ongoing: bool,

    // git repository name for cloning (e.g. "my-repo" for /extras/my-repo.git)
    #[facet(default)]
    pub git_repo: Option<String>,
}
