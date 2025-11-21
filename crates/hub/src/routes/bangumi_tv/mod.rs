pub mod subject_blogs;
pub mod subject_comments;
pub mod subject_ep;
pub mod subject_topics;
pub mod user_blog;
pub mod user_collections;

/// Shared constants for Bangumi.tv routes.
pub(crate) const API_ROOT: &str = "https://api.bgm.tv";
pub(crate) const WEB_ROOT: &str = "https://bgm.tv";

/// Choose between original and localized title for Bangumi subjects.
pub(crate) fn local_name(en: &str, cn: &str, show_original: bool) -> String {
    if show_original {
        if !en.trim().is_empty() {
            en.trim().to_string()
        } else {
            cn.trim().to_string()
        }
    } else if !cn.trim().is_empty() {
        cn.trim().to_string()
    } else {
        en.trim().to_string()
    }
}
