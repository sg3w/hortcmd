//! Versioned VFS locations. URI decoding happens exactly once, at input boundaries.
use super::Scheme;
use crate::errors::{code, AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, Ipv6Addr};
use ts_rs::TS;

fn invalid(reason: &str) -> AppError {
    AppError::new(code::VALIDATION_BAD_INPUT)
        .op("vfs-location")
        .with("reason", reason)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum LocalFlavor {
    Posix,
    Windows,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
#[serde(deny_unknown_fields)]
pub struct LocalPath {
    pub flavor: LocalFlavor,
    pub raw: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
#[serde(deny_unknown_fields)]
pub struct RemoteAuthority {
    pub host: String,
    pub user: Option<String>,
    pub port: Option<u16>,
}

/// Decoded components, including empty components. Never a host-platform PathBuf.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
#[serde(deny_unknown_fields)]
pub struct ProviderPath {
    pub components: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub enum LocationPath {
    Local {
        path: LocalPath,
    },
    Remote {
        scheme: Scheme,
        authority: RemoteAuthority,
        path: ProviderPath,
    },
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct VfsLocation {
    version: u8,
    location: LocationPath,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct StoreIdentity(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct LocationIdentity(pub String);

pub fn encode(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if matches!(ch, '%' | '/' | '?' | '#' | '@' | ':' | '[' | ']') || ch.is_control() {
            let mut buf = [0; 4];
            for b in ch.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        } else {
            out.push(ch);
        }
    }
    out
}
fn decode(value: &str) -> AppResult<String> {
    let mut bytes = Vec::new();
    let raw = value.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%' {
            let hex = raw.get(i + 1..i + 3).ok_or_else(|| invalid("escape"))?;
            let text = std::str::from_utf8(hex).map_err(|_| invalid("escape"))?;
            bytes.push(u8::from_str_radix(text, 16).map_err(|_| invalid("escape"))?);
            i += 3;
        } else {
            bytes.push(raw[i]);
            i += 1;
        }
    }
    let value = String::from_utf8(bytes).map_err(|_| invalid("utf8"))?;
    if value.chars().any(char::is_control) {
        return Err(invalid("control"));
    }
    Ok(value)
}

impl ProviderPath {
    pub fn from_raw(raw: &str) -> AppResult<Self> {
        if !raw.starts_with('/') || raw.len() > 32768 || raw.chars().any(char::is_control) {
            return Err(invalid("provider-path"));
        }
        let components: Vec<_> = raw[1..].split('/').map(str::to_owned).collect();
        if components.iter().any(|c| c == "." || c == "..") {
            return Err(invalid("traversal"));
        }
        Ok(Self { components })
    }
    pub fn parse(raw: &str) -> AppResult<Self> {
        let mut components = Vec::new();
        if !raw.starts_with('/') {
            return Err(invalid("root"));
        }
        for part in raw[1..].split('/') {
            let component = decode(part)?;
            if component.contains('/') {
                return Err(invalid("encoded-separator"));
            }
            components.push(component);
        }
        let path = Self { components };
        Self::from_raw(&path.raw())
    }
    pub fn raw(&self) -> String {
        format!("/{}", self.components.join("/"))
    }
    pub fn uri(&self) -> String {
        format!(
            "/{}",
            self.components
                .iter()
                .map(|s| encode(s))
                .collect::<Vec<_>>()
                .join("/")
        )
    }
    pub fn parent(&self) -> Self {
        let mut parts = self.components.clone();
        // One final empty component is the directory delimiter. Earlier empty
        // components are resource identity, not redundant separators.
        if parts.last().is_some_and(String::is_empty) {
            parts.pop();
        }
        parts.pop();
        if parts.is_empty() || parts.last().is_some_and(String::is_empty) {
            parts.push(String::new());
        }
        Self { components: parts }
    }
    pub fn join(&self, name: &str) -> AppResult<Self> {
        if name.is_empty() || name.contains('/') || name == "." || name == ".." {
            return Err(invalid("component"));
        }
        let raw = self.raw();
        Self::from_raw(&format!(
            "{}{name}",
            if raw.ends_with('/') {
                raw
            } else {
                format!("{raw}/")
            }
        ))
    }
    pub fn contains(&self, other: &Self) -> bool {
        let mut root = self.components.as_slice();
        if root.last().is_some_and(String::is_empty) {
            root = &root[..root.len() - 1];
        }
        other.components.starts_with(root)
    }
}

impl RemoteAuthority {
    pub fn parse(raw: &str) -> AppResult<Self> {
        if raw.chars().any(char::is_control) {
            return Err(invalid("control"));
        }
        let (user, endpoint) = match raw.split_once('@') {
            Some((user, endpoint)) => {
                if user.contains(':') || endpoint.contains('@') {
                    return Err(invalid("credentials"));
                }
                let user = decode(user)?;
                if user.contains(':') {
                    return Err(invalid("credentials"));
                }
                (Some(user), endpoint)
            }
            None => (None, raw),
        };
        let (host, port) = if let Some(rest) = endpoint.strip_prefix('[') {
            let (host, tail) = rest.split_once(']').ok_or_else(|| invalid("ipv6"))?;
            let parsed = host.parse::<Ipv6Addr>().map_err(|_| invalid("ipv6"))?;
            let host = if parsed.to_ipv4_mapped().is_some() {
                let parts = parsed.segments();
                format!("::ffff:{:x}:{:x}", parts[6], parts[7])
            } else {
                parsed.to_string()
            };
            (
                host,
                if tail.is_empty() {
                    None
                } else {
                    Some(tail.strip_prefix(':').ok_or_else(|| invalid("port"))?)
                },
            )
        } else {
            let (host, port) = endpoint
                .split_once(':')
                .map_or((endpoint, None), |(h, p)| (h, Some(p)));
            if host.is_empty()
                || host.chars().any(|c| {
                    c.is_whitespace() || matches!(c, '%' | '/' | '\\' | '?' | '#' | '[' | ']')
                })
                || host.chars().any(|c| {
                    c.is_ascii() && !c.is_ascii_alphanumeric() && !matches!(c, '.' | '-' | '_')
                })
            {
                return Err(invalid("host"));
            }
            let host = host
                .parse::<Ipv4Addr>()
                .map(|h| h.to_string())
                .unwrap_or_else(|_| host.to_lowercase());
            (host, port)
        };
        let port = port
            .map(|p| {
                if !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()) {
                    p.parse::<u16>()
                } else {
                    "".parse::<u16>()
                }
                .ok()
                .filter(|p| *p != 0)
                .ok_or_else(|| invalid("port"))
            })
            .transpose()?;
        Ok(Self { host, user, port })
    }
    pub fn key(&self, scheme: Scheme) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.to_lowercase()
        };
        format!(
            "{}@{}:{}",
            encode(self.user.as_deref().unwrap_or_default()),
            host,
            self.port.or_else(|| scheme.default_port()).unwrap_or(0)
        )
    }
    pub fn uri(&self) -> String {
        let user = self
            .user
            .as_ref()
            .map(|u| format!("{}@", encode(u)))
            .unwrap_or_default();
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        format!(
            "{user}{host}{}",
            self.port.map(|p| format!(":{p}")).unwrap_or_default()
        )
    }
}

impl VfsLocation {
    pub fn kind(&self) -> &LocationPath {
        &self.location
    }
    pub fn scheme(&self) -> Scheme {
        match self.location {
            LocationPath::Local { .. } => Scheme::Local,
            LocationPath::Remote { scheme, .. } => scheme,
        }
    }
    pub fn path(&self) -> String {
        match &self.location {
            LocationPath::Local { path } => path.raw.clone(),
            LocationPath::Remote { path, .. } => path.raw(),
        }
    }
    #[cfg(test)]
    pub fn host(&self) -> Option<&str> {
        match &self.location {
            LocationPath::Remote { authority, .. } => Some(&authority.host),
            _ => None,
        }
    }
    #[cfg(test)]
    pub fn user(&self) -> Option<&str> {
        match &self.location {
            LocationPath::Remote { authority, .. } => authority.user.as_deref(),
            _ => None,
        }
    }
    #[cfg(test)]
    pub fn port(&self) -> Option<u16> {
        match &self.location {
            LocationPath::Remote { authority, .. } => authority.port,
            _ => None,
        }
    }
    pub fn connection_key(&self) -> Option<String> {
        match &self.location {
            LocationPath::Remote {
                scheme, authority, ..
            } => Some(authority.key(*scheme)),
            _ => None,
        }
    }
    pub fn parse(raw: &str) -> AppResult<Self> {
        if raw.len() > 32768 || raw.chars().any(char::is_control) {
            return Err(invalid("input"));
        }
        let location = match raw.split_once("://") {
            Some((scheme, rest)) if !raw.starts_with('/') && !raw.contains(":\\") => {
                let scheme = Scheme::from_scheme_str(&scheme.to_lowercase())
                    .ok_or_else(|| invalid("scheme"))?;
                if scheme == Scheme::Local {
                    Self::local(rest)
                } else {
                    if rest.contains(['?', '#']) {
                        return Err(invalid("query-or-fragment"));
                    }
                    let (authority, path) = rest
                        .find('/')
                        .map_or((rest, "/"), |i| (&rest[..i], &rest[i..]));
                    let path = ProviderPath::parse(path)?;
                    if scheme == Scheme::Smb && path.raw().contains('\\') {
                        return Err(invalid("separator"));
                    }
                    LocationPath::Remote {
                        scheme,
                        authority: RemoteAuthority::parse(authority)?,
                        path,
                    }
                }
            }
            _ => Self::local(raw),
        };
        Ok(Self {
            version: 1,
            location,
        })
    }
    fn local(raw: &str) -> LocationPath {
        let windows = raw.starts_with("\\\\") || raw.as_bytes().get(1) == Some(&b':');
        LocationPath::Local {
            path: LocalPath {
                flavor: if windows {
                    LocalFlavor::Windows
                } else {
                    LocalFlavor::Posix
                },
                raw: raw.into(),
            },
        }
    }
    pub fn validate(&self) -> AppResult<()> {
        if self.version != 1 {
            return Err(invalid("version"));
        }
        if Self::parse(&self.uri())?.location != self.location {
            return Err(invalid("noncanonical"));
        }
        Ok(())
    }
    pub fn uri(&self) -> String {
        match &self.location {
            LocationPath::Local { path } => path.raw.clone(),
            LocationPath::Remote {
                scheme,
                authority,
                path,
            } => format!("{}://{}{}", scheme.as_str(), authority.uri(), path.uri()),
        }
    }
    pub fn store_identity(&self) -> StoreIdentity {
        StoreIdentity(match &self.location {
            LocationPath::Local { .. } => "local".into(),
            LocationPath::Remote {
                scheme, authority, ..
            } => format!("{}:{}", scheme.as_str(), authority.key(*scheme)),
        })
    }
    pub fn identity(&self) -> LocationIdentity {
        let path = match &self.location {
            LocationPath::Local { path } => path.raw.clone(),
            LocationPath::Remote { path, .. } => path.raw(),
        };
        LocationIdentity(format!("v1:{}\0{}", self.store_identity().0, path))
    }
}

/// Compatibility for persisted pre-v1 addresses: '%' was a literal byte.
pub fn migrate_legacy(raw: &str) -> AppResult<VfsLocation> {
    if let Some((scheme, rest)) = raw.split_once("://") {
        if matches!(scheme, "sftp" | "ftp" | "smb" | "webdav") {
            let (authority, path) = rest
                .find('/')
                .map_or((rest, "/"), |i| (&rest[..i], &rest[i..]));
            return VfsLocation::parse(&format!(
                "{scheme}://{authority}{}",
                ProviderPath::from_raw(path)?.uri()
            ));
        }
    }
    VfsLocation::parse(raw)
}

/// Provider adapters already hold a validated, canonical connection key.
pub fn remote_uri(scheme: &str, key: &str, raw_path: &str) -> AppResult<String> {
    Ok(format!(
        "{scheme}://{key}{}",
        ProviderPath::from_raw(raw_path)?.uri()
    ))
}

#[derive(Serialize, TS)]
#[ts(export, export_to = "../../src/ipc/bindings/")]
pub struct LocationView {
    pub location: VfsLocation,
    pub uri: String,
    pub identity: LocationIdentity,
    pub store: StoreIdentity,
    pub parent: Option<String>,
}

/// Explicit URI/legacy import boundary. No I/O or provider lookup is performed.
#[tauri::command]
pub fn vfs_location(raw: String, legacy: bool, child: Option<String>) -> AppResult<LocationView> {
    let mut location = if legacy {
        migrate_legacy(&raw)?
    } else {
        VfsLocation::parse(&raw)?
    };
    if let Some(child) = child {
        let LocationPath::Remote { path, .. } = &mut location.location else {
            return Err(invalid("remote-child-required"));
        };
        *path = path.join(&child)?;
    }
    location.validate()?;
    let parent = if let LocationPath::Remote { path, .. } = &location.location {
        let mut parent = location.clone();
        if let LocationPath::Remote { path: target, .. } = &mut parent.location {
            *target = path.parent();
        }
        Some(parent.uri())
    } else if let LocationPath::Local { path } = &location.location {
        Some(path.parent())
    } else {
        None
    };
    Ok(LocationView {
        uri: location.uri(),
        identity: location.identity(),
        store: location.store_identity(),
        parent,
        location,
    })
}

impl<'de> Deserialize<'de> for VfsLocation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            version: u8,
            location: LocationPath,
        }
        let wire = Wire::deserialize(deserializer)?;
        let location = Self {
            version: wire.version,
            location: wire.location,
        };
        location.validate().map_err(serde::de::Error::custom)?;
        Ok(location)
    }
}

impl PartialEq for VfsLocation {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version && self.identity() == other.identity()
    }
}
impl Eq for VfsLocation {}
impl std::hash::Hash for VfsLocation {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&(self.version, self.identity()), state);
    }
}
