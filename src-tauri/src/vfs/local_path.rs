//! Lexical local-path presentation. Never used as filesystem authorization.
use super::location::{LocalFlavor, LocalPath};
impl LocalPath {
    fn separator(&self, ch: char) -> bool {
        ch == '/' || (self.flavor == LocalFlavor::Windows && ch == '\\')
    }
    fn root_len(&self) -> usize {
        let raw = self.raw.as_str();
        if self.flavor == LocalFlavor::Posix {
            return usize::from(raw.starts_with('/'));
        }
        let drive_at = if raw.starts_with(r"\\?\") { 4 } else { 0 };
        if raw.as_bytes().get(drive_at + 1) == Some(&b':') {
            return drive_at + 2 + usize::from(raw[drive_at + 2..].starts_with(['/', '\\']));
        }
        let unc_at = if raw.starts_with(r"\\?\UNC\") {
            8
        } else if raw.starts_with(r"\\") {
            2
        } else {
            0
        };
        if unc_at > 0 {
            let mut separators = raw[unc_at..]
                .char_indices()
                .filter(|(_, c)| self.separator(*c));
            separators.next();
            return separators.next().map_or(raw.len(), |(i, _)| unc_at + i);
        }
        0
    }
    pub fn parent(&self) -> String {
        let root = self.root_len();
        let trimmed = self.raw.trim_end_matches(|c| self.separator(c));
        if trimmed.len() <= root {
            return self.raw[..root].to_owned();
        }
        let at = trimmed.rfind(|c| self.separator(c));
        match at {
            Some(i) if i >= root => trimmed[..i].to_owned(),
            _ if root > 0 => self.raw[..root].to_owned(),
            _ => ".".into(),
        }
    }
}
