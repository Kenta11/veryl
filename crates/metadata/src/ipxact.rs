use crate::Metadata;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// IP-XACT version emitted by `veryl ipxact`.
///
/// Each variant corresponds to a published IEEE 1685 schema. The XML namespace
/// and the layout of some elements (most notably `vectors`/`moduleParameters`)
/// differ between versions, so the emitter branches on this value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum IpxactVersion {
    /// IEEE 1685-2009 (SPIRIT namespace).
    #[serde(rename = "2009")]
    V2009,
    /// IEEE 1685-2014 (Accellera namespace). Default, widest tool support.
    #[default]
    #[serde(rename = "2014")]
    V2014,
    /// IEEE 1685-2022 (Accellera namespace).
    #[serde(rename = "2022")]
    V2022,
}

impl IpxactVersion {
    /// XML namespace URI for this IP-XACT version.
    pub fn namespace(&self) -> &'static str {
        match self {
            IpxactVersion::V2009 => {
                "http://www.spiritconsortium.org/XMLSchema/SPIRIT/1685-2009"
            }
            IpxactVersion::V2014 => "http://www.accellera.org/XMLSchema/IPXACT/1685-2014",
            IpxactVersion::V2022 => "http://www.accellera.org/XMLSchema/IPXACT/1685-2022",
        }
    }

    /// XML namespace prefix. 1685-2009 uses `spirit`; later revisions use `ipxact`.
    pub fn prefix(&self) -> &'static str {
        match self {
            IpxactVersion::V2009 => "spirit",
            IpxactVersion::V2014 | IpxactVersion::V2022 => "ipxact",
        }
    }

    /// `xsi:schemaLocation` value pairing the namespace with its index schema.
    pub fn schema_location(&self) -> String {
        let ns = self.namespace();
        format!("{ns} {ns}/index.xsd")
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ipxact {
    /// Output directory for the generated `*.xml` component files.
    #[serde(default = "default_path")]
    pub path: PathBuf,

    /// IP-XACT schema version to emit.
    #[serde(default)]
    pub version: IpxactVersion,

    /// VLNV vendor field. Falls back to a value derived from the project when
    /// unset (see `Metadata::ipxact_vendor`).
    #[serde(default)]
    pub vendor: Option<String>,

    /// VLNV library field. Falls back to the project name when unset.
    #[serde(default)]
    pub library: Option<String>,

    /// VLNV version field. Falls back to the project version when unset.
    #[serde(default)]
    pub component_version: Option<String>,
}

impl Default for Ipxact {
    fn default() -> Self {
        Self {
            path: default_path(),
            version: IpxactVersion::default(),
            vendor: None,
            library: None,
            component_version: None,
        }
    }
}

fn default_path() -> PathBuf {
    PathBuf::from("ipxact")
}

impl Metadata {
    /// Resolved VLNV vendor for IP-XACT output.
    ///
    /// Uses `[ipxact] vendor` when set. Otherwise derives a vendor from the
    /// project repository host (e.g. `github.com` from a repository URL), and
    /// finally falls back to the project name.
    pub fn ipxact_vendor(&self) -> String {
        if let Some(vendor) = &self.ipxact.vendor {
            return vendor.clone();
        }
        if let Some(repo) = &self.project.repository {
            if let Some(host) = repository_host(repo) {
                return host;
            }
        }
        self.project.name.clone()
    }

    /// Resolved VLNV library for IP-XACT output. Defaults to the project name.
    pub fn ipxact_library(&self) -> String {
        self.ipxact
            .library
            .clone()
            .unwrap_or_else(|| self.project.name.clone())
    }

    /// Resolved VLNV version for IP-XACT output. Defaults to the project
    /// version, or `0.0.0` when the project has no version.
    pub fn ipxact_component_version(&self) -> String {
        if let Some(version) = &self.ipxact.component_version {
            return version.clone();
        }
        self.project
            .version
            .as_ref()
            .map(|x| x.to_string())
            .unwrap_or_else(|| "0.0.0".to_string())
    }
}

/// Extract the host portion of a repository URL for use as an IP-XACT vendor.
///
/// Handles both `https://host/...` and `git@host:...` forms. Returns `None`
/// when no host can be determined.
fn repository_host(repo: &str) -> Option<String> {
    if let Some(rest) = repo
        .strip_prefix("https://")
        .or_else(|| repo.strip_prefix("http://"))
        .or_else(|| repo.strip_prefix("git://"))
        .or_else(|| repo.strip_prefix("ssh://"))
    {
        let host = rest.split(['/', ':']).next()?;
        let host = host.split('@').next_back()?;
        if host.is_empty() {
            None
        } else {
            Some(host.to_string())
        }
    } else if let Some(rest) = repo.strip_prefix("git@") {
        let host = rest.split(':').next()?;
        if host.is_empty() {
            None
        } else {
            Some(host.to_string())
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_namespace_and_prefix() {
        assert_eq!(IpxactVersion::V2009.prefix(), "spirit");
        assert_eq!(IpxactVersion::V2014.prefix(), "ipxact");
        assert_eq!(IpxactVersion::V2022.prefix(), "ipxact");
        assert!(IpxactVersion::V2009.namespace().contains("SPIRIT/1685-2009"));
        assert!(IpxactVersion::V2014.namespace().contains("IPXACT/1685-2014"));
        assert!(IpxactVersion::V2022.namespace().contains("IPXACT/1685-2022"));
        assert!(
            IpxactVersion::V2014
                .schema_location()
                .ends_with("index.xsd")
        );
    }

    #[test]
    fn default_version_is_2014() {
        assert_eq!(IpxactVersion::default(), IpxactVersion::V2014);
    }

    #[test]
    fn repository_host_extraction() {
        assert_eq!(
            repository_host("https://github.com/foo/bar"),
            Some("github.com".to_string())
        );
        assert_eq!(
            repository_host("git@gitlab.com:foo/bar.git"),
            Some("gitlab.com".to_string())
        );
        assert_eq!(
            repository_host("ssh://git@example.org/foo"),
            Some("example.org".to_string())
        );
        assert_eq!(repository_host("not a url"), None);
    }
}
