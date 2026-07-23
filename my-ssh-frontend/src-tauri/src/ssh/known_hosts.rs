use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrustedHostKey {
    pub algorithm: String,
    pub fingerprint: String,
    pub trusted_at: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct KnownHostsDocument {
    #[serde(default)]
    hosts: HashMap<String, TrustedHostKey>,
}

pub struct KnownHosts {
    path: PathBuf,
    hosts: HashMap<String, TrustedHostKey>,
}

impl KnownHosts {
    pub fn open(app_dir: PathBuf) -> Result<Self, std::io::Error> {
        let path = app_dir.join("known_hosts.json");
        let hosts = match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str::<KnownHostsDocument>(&contents)
                .map(|document| document.hosts)
                .unwrap_or_else(|error| {
                    log::warn!("Ignoring malformed known_hosts file: {}", error);
                    HashMap::new()
                }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => HashMap::new(),
            Err(error) => return Err(error),
        };
        Ok(Self { path, hosts })
    }

    pub fn host_id(host: &str, port: u16) -> String {
        format!("{}:{}", host.trim().to_ascii_lowercase(), port)
    }

    pub fn get(&self, host: &str, port: u16) -> Option<&TrustedHostKey> {
        self.hosts.get(&Self::host_id(host, port))
    }

    pub fn trust(
        &mut self,
        host: &str,
        port: u16,
        algorithm: String,
        fingerprint: String,
    ) -> Result<(), std::io::Error> {
        self.hosts.insert(
            Self::host_id(host, port),
            TrustedHostKey {
                algorithm,
                fingerprint,
                trusted_at: chrono::Utc::now().to_rfc3339(),
            },
        );
        self.save()
    }

    fn save(&self) -> Result<(), std::io::Error> {
        let contents = serde_json::to_vec_pretty(&KnownHostsDocument {
            hosts: self.hosts.clone(),
        })
        .map_err(std::io::Error::other)?;
        let temporary_path = self.path.with_extension("json.tmp");
        let mut file = fs::File::create(&temporary_path)?;
        file.write_all(&contents)?;
        file.sync_all()?;
        fs::rename(temporary_path, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::KnownHosts;

    #[test]
    fn normalizes_host_identity() {
        assert_eq!(KnownHosts::host_id(" Example.COM ", 22), "example.com:22");
    }
}
