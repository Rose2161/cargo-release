use std::collections::BTreeMap;
use std::path::Path;

use crate::config::Replace;
use crate::error::CargoResult;

pub static NOW: once_cell::sync::Lazy<String> = once_cell::sync::Lazy::new(|| {
    time::OffsetDateTime::now_utc()
        .format(time::macros::format_description!("[year]-[month]-[day]"))
        .unwrap()
});

#[derive(Clone, Default, Debug)]
pub struct Template<'a> {
    pub prev_version: Option<&'a str>,
    pub prev_metadata: Option<&'a str>,
    pub version: Option<&'a str>,
    pub metadata: Option<&'a str>,
    pub crate_name: Option<&'a str>,
    pub date: Option<&'a str>,

    pub prefix: Option<&'a str>,
    pub tag_name: Option<&'a str>,
}

impl<'a> Template<'a> {
    pub fn render(&self, input: &str) -> String {
        let mut s = input.to_string();
        const PREV_VERSION: &str = "{{prev_version}}";
        if let Some(prev_version) = self.prev_version {
            s = s.replace(PREV_VERSION, prev_version);
        }
        const PREV_METADATA: &str = "{{prev_metadata}}";
        if let Some(prev_metadata) = self.prev_metadata {
            s = s.replace(PREV_METADATA, prev_metadata);
        }
        const VERSION: &str = "{{version}}";
        if let Some(version) = self.version {
            s = s.replace(VERSION, version);
        }
        const METADATA: &str = "{{metadata}}";
        if let Some(metadata) = self.metadata {
            s = s.replace(METADATA, metadata);
        }
        const CRATE_NAME: &str = "{{crate_name}}";
        if let Some(crate_name) = self.crate_name {
            s = s.replace(CRATE_NAME, crate_name);
        }
        const DATE: &str = "{{date}}";
        if let Some(date) = self.date {
            s = s.replace(DATE, date);
        }

        const PREFIX: &str = "{{prefix}}";
        if let Some(prefix) = self.prefix {
            s = s.replace(PREFIX, prefix);
        }
        const TAG_NAME: &str = "{{tag_name}}";
        if let Some(tag_name) = self.tag_name {
            s = s.replace(TAG_NAME, tag_name);
        }
        s
    }
}

pub fn do_file_replacements(
    replace_config: &[Replace],
    template: &Template<'_>,
    cwd: &Path,
    prerelease: bool,
    noisy: bool,
    dry_run: bool,
) -> CargoResult<bool> {
    // Since we don't have a convenient insert-order map, let's do sorted, rather than random.
    let mut by_file = BTreeMap::new();
    for replace in replace_config {
        let file = replace.file.clone();
        by_file.entry(file).or_insert_with(Vec::new).push(replace);
    }

    for (path, replaces) in by_file.into_iter() {
        let file = cwd.join(&path);
        log::debug!("processing replacements for file {}", file.display());
        if !file.exists() {
            anyhow::bail!("unable to find file {} to perform replace", file.display());
        }
        let data = std::fs::read_to_string(&file)?;
        let mut replaced = data.clone();

        for replace in replaces {
            if prerelease && !replace.prerelease {
                log::debug!("pre-release, not replacing {}", replace.search);
                continue;
            }

            let pattern = replace.search.as_str();
            let r = regex::RegexBuilder::new(pattern).multi_line(true).build()?;

            let min = replace.min.or(replace.exactly).unwrap_or(1);
            let max = replace.max.or(replace.exactly).unwrap_or(std::usize::MAX);
            let actual = r.find_iter(&replaced).count();
            if actual < min {
                anyhow::bail!(
                    "for `{}` in '{}', at least {} replacements expected, found {}",
                    pattern,
                    path.display(),
                    min,
                    actual
                );
            } else if max < actual {
                anyhow::bail!(
                    "for `{}` in '{}', at most {} replacements expected, found {}",
                    pattern,
                    path.display(),
                    max,
                    actual
                );
            }

            let to_replace = replace.replace.as_str();
            let replacer = template.render(to_replace);

            replaced = r.replace_all(&replaced, replacer.as_str()).into_owned();
        }

        if data != replaced {
            if dry_run {
                let display_path = path.display().to_string();
                let data_lines: Vec<_> = data.lines().map(|s| format!("{}\n", s)).collect();
                let replaced_lines: Vec<_> = replaced.lines().map(|s| format!("{}\n", s)).collect();
                let diff = difflib::unified_diff(
                    &data_lines,
                    &replaced_lines,
                    display_path.as_str(),
                    display_path.as_str(),
                    "original",
                    "replaced",
                    0,
                );
                if noisy {
                    let _ = crate::ops::shell::status(
                        "Replacing",
                        format!(
                            "in {}\n{}",
                            path.display(),
                            itertools::join(diff.into_iter(), "")
                        ),
                    );
                } else {
                    let _ =
                        crate::ops::shell::status("Replacing", format!("in {}", path.display()));
                }
            } else {
                std::fs::write(&file, replaced)?;
            }
        } else {
            log::trace!("{} is unchanged", file.display());
        }
    }
    Ok(true)
}
