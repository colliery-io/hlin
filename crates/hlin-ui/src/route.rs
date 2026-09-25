//! The shell's own addresses for platform pages, and which pages there are
//! (specification HLIN-S-0007, *Pages*).
//!
//! A navigation entry that declares a module opens at full width in the shell,
//! on an address of the shell's own, so a person can send it to somebody or
//! reload it and land on the same page. That address is
//! `/page/{platform}/{path}`: beside `/s/{layout}`, and claimed by nothing
//! else the shell serves. `/p/` is the request proxy and `/m/` the module
//! assets, and a path segment is matched whole, so `/page/` is neither.
//!
//! Free of the DOM, and tested on the host, like [`crate::bridge`]: what an
//! address means is the part worth getting exactly right, because a link that
//! opens the wrong page, or none, fails silently in somebody else's browser.

use hlin_stream::layout::{CatalogPlatform, CatalogUi, ModuleLimits};

/// Where every page's address starts.
pub const PAGE_PREFIX: &str = "/page/";

/// A page, as its address names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageTarget {
    /// The platform whose navigation entry it is.
    pub platform: String,
    /// The entry's path.
    pub path: String,
}

/// A page this shell can open: a navigation entry whose module it can host.
#[derive(Debug, Clone, PartialEq)]
pub struct PageEntry {
    /// The platform whose entry it is.
    pub platform: String,
    /// The platform's display name, or its id where it has none.
    pub platform_name: String,
    /// What a person reads.
    pub label: String,
    /// The entry's path.
    pub path: String,
    /// The module that draws it.
    pub ui: CatalogUi,
    /// The limits the platform's modules run within, where it said.
    pub limits: Option<ModuleLimits>,
}

impl PageEntry {
    /// Where this page is, as an address.
    pub fn target(&self) -> PageTarget {
        PageTarget {
            platform: self.platform.clone(),
            path: self.path.clone(),
        }
    }
}

/// The shell's address for a page.
///
/// Each segment is percent-encoded and the path's own slashes are kept, so an
/// entry nested under its platform reads as one in the address bar.
pub fn page_address(target: &PageTarget) -> String {
    let path: Vec<String> = target.path.split('/').map(encode).collect();
    format!(
        "{PAGE_PREFIX}{}/{}",
        encode(&target.platform),
        path.join("/")
    )
}

/// The page an address names, if it names one.
///
/// Anything that is not a page's address, or does not decode, is `None`: the
/// shell then shows the surface, as it would for any address it does not know.
pub fn page_of(pathname: &str) -> Option<PageTarget> {
    let rest = pathname.strip_prefix(PAGE_PREFIX)?;
    let (platform, path) = rest.split_once('/')?;
    let platform = decode(platform)?;
    let path = path
        .split('/')
        .map(decode)
        .collect::<Option<Vec<_>>>()?
        .join("/");
    (!platform.is_empty() && !path.is_empty()).then_some(PageTarget { platform, path })
}

/// Every page this shell can open, a platform's together, in the catalogue's
/// order of platforms and each platform's own order within it: by weight, then
/// by label.
///
/// An entry without a module is not a page, and is left out: the shell has
/// never drawn those, and has nowhere of its own to send a person for one.
pub fn pages(catalog: &[CatalogPlatform]) -> Vec<PageEntry> {
    catalog
        .iter()
        .flat_map(|platform| {
            let mut own: Vec<PageEntry> = platform
                .navigation
                .iter()
                .filter_map(|entry| {
                    Some(PageEntry {
                        platform: platform.id.clone(),
                        platform_name: platform.name.clone().unwrap_or_else(|| platform.id.clone()),
                        label: entry.label.clone(),
                        path: entry.path.clone(),
                        ui: entry.ui.clone()?,
                        limits: platform.module_limits,
                    })
                })
                .collect();
            let weight = |page: &PageEntry| {
                platform
                    .navigation
                    .iter()
                    .find(|entry| entry.path == page.path)
                    .map_or(0, |entry| entry.weight)
            };
            own.sort_by(|left, right| {
                weight(left)
                    .cmp(&weight(right))
                    .then_with(|| left.label.cmp(&right.label))
            });
            own
        })
        .collect()
}

/// The page a target names, where this shell can open it.
pub fn page_entry(catalog: &[CatalogPlatform], target: &PageTarget) -> Option<PageEntry> {
    pages(catalog)
        .into_iter()
        .find(|page| page.platform == target.platform && page.path == target.path)
}

/// Percent-encode everything but the characters a path segment may carry as
/// they are.
fn encode(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// Undo [`encode`], or anything else that percent-encoded UTF-8. `None` for a
/// broken escape or bytes that are not UTF-8.
fn decode(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = segment.get(at + 1..at + 3)?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        } else {
            decoded.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hlin_stream::layout::CatalogNavigation;

    fn target(platform: &str, path: &str) -> PageTarget {
        PageTarget {
            platform: platform.to_string(),
            path: path.to_string(),
        }
    }

    fn entry(label: &str, path: &str, weight: i64, module: bool) -> CatalogNavigation {
        CatalogNavigation {
            label: label.to_string(),
            path: path.to_string(),
            icon: None,
            weight,
            ui: module.then(|| CatalogUi {
                entry: format!("/ui/{path}/index.html"),
                bridge: 1,
            }),
        }
    }

    fn platform(id: &str, navigation: Vec<CatalogNavigation>) -> CatalogPlatform {
        CatalogPlatform {
            id: id.to_string(),
            name: Some(id.to_uppercase()),
            reachable: true,
            panels: vec![],
            module_limits: None,
            navigation,
        }
    }

    #[test]
    fn a_page_has_an_address_of_the_shells_own() {
        assert_eq!(
            page_address(&target("checklist", "lists")),
            "/page/checklist/lists"
        );
        // Nothing the proxy or the assets answer.
        assert!(!page_address(&target("p", "m")).starts_with("/p/"));
        assert!(!page_address(&target("p", "m")).starts_with("/m/"));
    }

    #[test]
    fn an_address_names_the_page_it_was_made_from() {
        for (platform, path) in [
            ("checklist", "lists"),
            ("checklist", "lists/team"),
            ("orebank", "a page with spaces & ünïcode"),
            ("orebank", "/leading"),
            ("orebank", "trailing/"),
            ("odd/id", "100%"),
        ] {
            let target = target(platform, path);
            assert_eq!(page_of(&page_address(&target)), Some(target));
        }
    }

    #[test]
    fn anything_else_names_no_page() {
        for address in [
            "/",
            "/s/7f3c",
            "/p/checklist/api/lists",
            "/m/checklist/ui/index.html",
            "/page/",
            "/page/checklist",
            "/page/checklist/",
            "/page//lists",
            "/page/checklist/%zz",
            "/page/checklist/%ff",
            "/pages/checklist/lists",
        ] {
            assert_eq!(page_of(address), None, "{address}");
        }
    }

    #[test]
    fn only_entries_with_a_module_are_pages_in_each_platforms_own_order() {
        let catalog = vec![
            platform(
                "orebank",
                vec![
                    entry("Overview", "overview", 0, false),
                    entry("Zeta", "zeta", 1, true),
                    entry("Beta", "beta", 1, true),
                    entry("First", "first", -5, true),
                ],
            ),
            platform("checklist", vec![entry("Lists", "lists", 0, true)]),
            platform("quiet", vec![]),
        ];
        let listed: Vec<(String, String, String)> = pages(&catalog)
            .into_iter()
            .map(|page| (page.platform, page.platform_name, page.path))
            .collect();
        assert_eq!(
            listed,
            vec![
                ("orebank".into(), "OREBANK".into(), "first".into()),
                ("orebank".into(), "OREBANK".into(), "beta".into()),
                ("orebank".into(), "OREBANK".into(), "zeta".into()),
                ("checklist".into(), "CHECKLIST".into(), "lists".into()),
            ]
        );

        assert!(page_entry(&catalog, &target("checklist", "lists")).is_some());
        assert_eq!(
            page_entry(&catalog, &target("orebank", "overview")),
            None,
            "a plain link is not a page"
        );
        assert_eq!(page_entry(&catalog, &target("nobody", "lists")), None);
    }
}
