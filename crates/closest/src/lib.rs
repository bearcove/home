use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::Hash;

#[derive(Debug, Clone, Copy)]
pub enum ResourceKind {
    Tag,
    Page,
    Media,
    Series,
    Route,
    Input,
    Template,
    Shortcode,
    AssetRoute,
}

impl ResourceKind {
    pub fn as_snake_case(&self) -> String {
        let debug_str = format!("{self:?}");
        debug_str
            .chars()
            .enumerate()
            .fold(String::new(), |mut acc, (i, c)| {
                if i > 0 && c.is_uppercase() {
                    acc.push('_');
                }
                acc.push(c.to_ascii_lowercase());
                acc
            })
    }
}

pub trait Closest {
    /// Returns the closest key to the given key, by levenshtein distance
    fn closest(self, key: impl AsRef<str>) -> Option<String>;
}

impl<'a, Iter, T: AsRef<str> + 'static> Closest for Iter
where
    Iter: Iterator<Item = &'a T>,
{
    fn closest(self, key: impl AsRef<str>) -> Option<String> {
        self.map(|k| k.as_ref())
            .min_by(|a, b| {
                strsim::levenshtein(a, key.as_ref()).cmp(&strsim::levenshtein(b, key.as_ref()))
            })
            .map(|s| s.to_owned())
    }
}

pub trait GetOrHelp<K, V> {
    fn get_or_help<Q>(&self, kind: ResourceKind, key: &Q) -> Result<&V, ClosestError>
    where
        K: Borrow<Q>,
        Q: AsRef<str> + Hash + Eq + ?Sized;
}

/// An error message indicating which resource was closest to the one you were looking for
#[derive(Debug)]
pub struct ClosestError(String);

impl ClosestError {
    pub fn new(message: String) -> Self {
        Self(message)
    }
}

impl std::fmt::Display for ClosestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ClosestError {}

impl<K, V> GetOrHelp<K, V> for HashMap<K, V>
where
    K: AsRef<str> + std::hash::Hash + Eq + 'static,
{
    fn get_or_help<Q>(&self, kind: ResourceKind, key: &Q) -> Result<&V, ClosestError>
    where
        K: Borrow<Q>,
        Q: AsRef<str> + Hash + Eq + ?Sized,
    {
        let result = self.get(key);
        if let Some(value) = result {
            Ok(value)
        } else {
            let closest = self.keys().closest(key.as_ref());
            let error_message = match closest {
                Some(closest_key) => format!(
                    "Could not find {} with key \x1b[31m{}\x1b[0m. Did you mean \x1b[32m{}\x1b[0m?",
                    kind.as_snake_case(),
                    key.as_ref(),
                    closest_key
                ),
                None => format!(
                    "Could not find {} with key \x1b[31m{}\x1b[0m. (And no close matches)",
                    kind.as_snake_case(),
                    key.as_ref()
                ),
            };
            Err(ClosestError(error_message))
        }
    }
}
