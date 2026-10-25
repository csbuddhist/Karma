#![forbid(unsafe_code)]
//! Signed in-place updates.
//!
//! A release bundle carries `release-manifest.json`, listing the SHA-256 of
//! every file, and `release-manifest.json.sig`, an Ed25519 signature over the
//! exact manifest bytes. The service only stages a bundle whose signature
//! verifies against [`RELEASE_PUBLIC_KEY`] and whose version is strictly newer
//! than the running one, so neither a forged nor an older build can replace
//! the installed protection.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Ed25519 public key that signs Karma release manifests.
/// Hex: `f7a491cbfa22c9d91cdbcb523c2c2835e992f917e32394cdf5488b75aa999090`.
pub const RELEASE_PUBLIC_KEY: [u8; 32] = [
    0xf7, 0xa4, 0x91, 0xcb, 0xfa, 0x22, 0xc9, 0xd9, 0x1c, 0xdb, 0xcb, 0x52, 0x3c, 0x2c, 0x28, 0x35,
    0xe9, 0x92, 0xf9, 0x17, 0xe3, 0x23, 0x94, 0xcd, 0xf5, 0x48, 0x8b, 0x75, 0xaa, 0x99, 0x90, 0x90,
];
pub const PRODUCT: &str = "karma";
pub const MANIFEST_FORMAT: u32 = 1;
pub const MANIFEST_FILE: &str = "release-manifest.json";
pub const SIGNATURE_FILE: &str = "release-manifest.json.sig";
/// Directory inside the release ZIP that holds the bundle.
pub const BUNDLE_PREFIX: &str = "windows-x64-test/";
const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 1024;
const MAX_FILES: usize = 512;
const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 768 * 1024 * 1024;
pub const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_RELEASES_BYTES: u64 = 4 * 1024 * 1024;
const MAX_PATH_COMPONENTS: usize = 8;
const MAX_COMPONENT_CHARS: usize = 128;
const HTTP_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error("update storage failed")]
    Io(#[from] io::Error),
    #[error("release manifest is invalid")]
    InvalidManifest,
    #[error("release signature does not verify")]
    BadSignature,
    #[error("release {offered} is not newer than the installed {installed}")]
    NotNewer {
        offered: Version,
        installed: Version,
    },
    #[error("release file does not match its manifest: {0}")]
    HashMismatch(String),
    #[error("release file is missing: {0}")]
    MissingFile(String),
    #[error("release exceeds the size limit")]
    TooLarge,
    #[error("release archive is invalid")]
    InvalidArchive,
    #[error("release download failed")]
    Network,
    #[error("update path is not an ordinary file or directory")]
    UnsafePath,
}

/// A `MAJOR.MINOR.PATCH` product version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl FromStr for Version {
    type Err = UpdateError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut parts = value.split('.');
        let mut next = || {
            parts
                .next()
                .filter(|part| {
                    !part.is_empty()
                        && part.len() <= 6
                        && part.bytes().all(|byte| byte.is_ascii_digit())
                })
                .and_then(|part| part.parse().ok())
                .ok_or(UpdateError::InvalidManifest)
        };
        let version = Self {
            major: next()?,
            minor: next()?,
            patch: next()?,
        };
        if parts.next().is_some() {
            return Err(UpdateError::InvalidManifest);
        }
        Ok(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// The signed list of files in one release bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseManifest {
    pub format: u32,
    pub product: String,
    pub version: String,
    /// Bundle-relative path (forward slashes) to lowercase hex SHA-256.
    pub files: BTreeMap<String, String>,
}

impl ReleaseManifest {
    pub fn version(&self) -> Version {
        self.version
            .parse()
            .expect("verified manifests carry a valid version")
    }
}

/// A verified bundle copied into a staging directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedUpdate {
    pub manifest: ReleaseManifest,
    pub directory: PathBuf,
}

/// Parses `public_key` into an Ed25519 verifying key.
pub fn verifying_key(public_key: &[u8; 32]) -> Result<VerifyingKey, UpdateError> {
    VerifyingKey::from_bytes(public_key).map_err(|_| UpdateError::BadSignature)
}

/// Verifies the signature over the exact manifest bytes, then the manifest's
/// structure: product, version, and every path and hash.
pub fn verify_manifest(
    manifest_bytes: &[u8],
    signature_text: &str,
    public_key: &[u8; 32],
) -> Result<ReleaseManifest, UpdateError> {
    if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(UpdateError::TooLarge);
    }
    let signature = decode_hex(signature_text.trim())
        .and_then(|bytes| <[u8; 64]>::try_from(bytes).ok())
        .map(|bytes| Signature::from_bytes(&bytes))
        .ok_or(UpdateError::BadSignature)?;
    verifying_key(public_key)?
        .verify_strict(manifest_bytes, &signature)
        .map_err(|_| UpdateError::BadSignature)?;
    let manifest: ReleaseManifest =
        serde_json::from_slice(manifest_bytes).map_err(|_| UpdateError::InvalidManifest)?;
    if manifest.format != MANIFEST_FORMAT
        || manifest.product != PRODUCT
        || manifest.version.parse::<Version>().is_err()
        || manifest.files.is_empty()
        || manifest.files.len() > MAX_FILES
        || manifest.files.iter().any(|(path, hash)| {
            safe_relative_path(path).is_none()
                || path == MANIFEST_FILE
                || path == SIGNATURE_FILE
                || hash.len() != 64
                || !hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
    {
        return Err(UpdateError::InvalidManifest);
    }
    // NTFS resolves names without case sensitivity. Distinct signed entries
    // must never alias, nor may a file also be another entry's parent directory.
    let mut names = BTreeSet::new();
    for path in manifest.files.keys() {
        let name = path.to_ascii_lowercase();
        if !names.insert(name.clone()) || name == MANIFEST_FILE || name == SIGNATURE_FILE {
            return Err(UpdateError::InvalidManifest);
        }
        for (index, _) in name.match_indices('/') {
            if names.contains(&name[..index]) {
                return Err(UpdateError::InvalidManifest);
            }
        }
    }
    for name in &names {
        if names
            .range(format!("{name}/")..)
            .next()
            .is_some_and(|other| other.starts_with(&format!("{name}/")))
        {
            return Err(UpdateError::InvalidManifest);
        }
    }
    Ok(manifest)
}

/// Signs `manifest_bytes`, returning the hex signature file contents.
pub fn sign_manifest(manifest_bytes: &[u8], signing_key: &SigningKey) -> String {
    encode_hex(&signing_key.sign(manifest_bytes).to_bytes())
}

/// Builds the manifest for every file under `bundle` except the manifest and
/// signature themselves (and Finder metadata).
pub fn build_manifest(bundle: &Path, version: Version) -> Result<ReleaseManifest, UpdateError> {
    let mut files = BTreeMap::new();
    collect_files(bundle, bundle, &mut files)?;
    let manifest = ReleaseManifest {
        format: MANIFEST_FORMAT,
        product: PRODUCT.into(),
        version: version.to_string(),
        files,
    };
    if manifest.files.is_empty() || manifest.files.len() > MAX_FILES {
        return Err(UpdateError::InvalidManifest);
    }
    Ok(manifest)
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, String>,
) -> Result<(), UpdateError> {
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        reject_links(&path)?;
        let name = entry.file_name();
        if name == ".DS_Store" {
            continue;
        }
        if entry.file_type()?.is_dir() {
            collect_files(root, &path, files)?;
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| UpdateError::InvalidManifest)?
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        if relative == MANIFEST_FILE || relative == SIGNATURE_FILE {
            continue;
        }
        if safe_relative_path(&relative).is_none() {
            return Err(UpdateError::InvalidManifest);
        }
        let mut file = File::open(&path)?;
        let (hash, _) = hash_reader(&mut file, &mut io::sink(), MAX_FILE_BYTES)?;
        files.insert(relative, hash);
    }
    Ok(())
}

/// Maps a manifest path onto the platform, rejecting absolute paths, parent
/// references, drive letters, backslashes, and control characters.
pub fn safe_relative_path(relative: &str) -> Option<PathBuf> {
    let components: Vec<&str> = relative.split('/').collect();
    if components.len() > MAX_PATH_COMPONENTS
        || components.iter().any(|component| {
            component.is_empty()
                || !component.is_ascii()
                || *component == "."
                || *component == ".."
                || component.chars().count() > MAX_COMPONENT_CHARS
                || component.chars().any(|character| {
                    character.is_control()
                        || matches!(character, '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
                })
                || component.ends_with('.')
                || component.ends_with(' ')
                || is_windows_device_name(component)
                || component.to_ascii_lowercase().starts_with(".karma-update-")
        })
    {
        return None;
    }
    Some(components.iter().collect())
}

fn is_windows_device_name(component: &str) -> bool {
    let base = component
        .split('.')
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(
        base.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ((base.starts_with("COM") || base.starts_with("LPT"))
        && matches!(base.as_bytes().get(3), Some(b'1'..=b'9'))
        && base.len() == 4)
}

/// Reject symlinks and Windows junctions/reparse points in every existing
/// component. Callers must additionally keep destination parents writable only
/// by SYSTEM: path checks alone cannot prevent a writable-parent rename race.
pub fn reject_links(path: &Path) -> Result<(), UpdateError> {
    for ancestor in path.ancestors() {
        let metadata = match fs::symlink_metadata(ancestor) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            return Err(UpdateError::UnsafePath);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.is_file() && metadata.nlink() != 1 {
                return Err(UpdateError::UnsafePath);
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(UpdateError::UnsafePath);
            }
        }
    }
    Ok(())
}

/// Verifies and copies a bundle directory (for example, one an installer
/// unpacked) into `staging`, hashing the copies so later changes to `source`
/// cannot alter what is applied.
pub fn stage_directory(
    source: &Path,
    staging: &Path,
    installed: Version,
    public_key: &[u8; 32],
) -> Result<StagedUpdate, UpdateError> {
    reject_links(source)?;
    let manifest_bytes = read_limited(&source.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let signature = read_limited(&source.join(SIGNATURE_FILE), MAX_SIGNATURE_BYTES)?;
    let manifest = verified_newer(&manifest_bytes, &signature, installed, public_key)?;
    prepare_staging(staging)?;
    let mut total = 0_u64;
    for (relative, expected) in &manifest.files {
        let path = safe_relative_path(relative).ok_or(UpdateError::InvalidManifest)?;
        reject_links(&source.join(&path))?;
        let mut input = File::open(source.join(&path))
            .map_err(|_| UpdateError::MissingFile(relative.clone()))?;
        total = total.saturating_add(write_verified(
            &mut input,
            &staging.join(&path),
            relative,
            expected,
            MAX_TOTAL_BYTES.saturating_sub(total),
        )?);
    }
    finish_staging(staging, &manifest_bytes, &signature, manifest)
}

/// Verifies and extracts a release ZIP into `staging`. Only files listed in
/// the signed manifest are extracted; every other entry is ignored.
pub fn stage_zip(
    archive: &Path,
    staging: &Path,
    installed: Version,
    public_key: &[u8; 32],
) -> Result<StagedUpdate, UpdateError> {
    let mut zip =
        zip::ZipArchive::new(File::open(archive)?).map_err(|_| UpdateError::InvalidArchive)?;
    let manifest_bytes = read_zip_entry(&mut zip, MANIFEST_FILE, MAX_MANIFEST_BYTES)?;
    let signature = read_zip_entry(&mut zip, SIGNATURE_FILE, MAX_SIGNATURE_BYTES)?;
    let manifest = verified_newer(&manifest_bytes, &signature, installed, public_key)?;
    prepare_staging(staging)?;
    let mut total = 0_u64;
    for (relative, expected) in &manifest.files {
        let path = safe_relative_path(relative).ok_or(UpdateError::InvalidManifest)?;
        let mut entry = zip
            .by_name(&format!("{BUNDLE_PREFIX}{relative}"))
            .map_err(|_| UpdateError::MissingFile(relative.clone()))?;
        if entry.size() > MAX_FILE_BYTES {
            return Err(UpdateError::TooLarge);
        }
        total = total.saturating_add(write_verified(
            &mut entry,
            &staging.join(&path),
            relative,
            expected,
            MAX_TOTAL_BYTES.saturating_sub(total),
        )?);
    }
    finish_staging(staging, &manifest_bytes, &signature, manifest)
}

fn verified_newer(
    manifest_bytes: &[u8],
    signature: &[u8],
    installed: Version,
    public_key: &[u8; 32],
) -> Result<ReleaseManifest, UpdateError> {
    let signature = std::str::from_utf8(signature).map_err(|_| UpdateError::BadSignature)?;
    let manifest = verify_manifest(manifest_bytes, signature, public_key)?;
    let offered = manifest.version();
    if offered <= installed {
        return Err(UpdateError::NotNewer { offered, installed });
    }
    Ok(manifest)
}

fn prepare_staging(staging: &Path) -> Result<(), UpdateError> {
    reject_links(staging)?;
    if staging.exists() {
        remove_directory(staging)?;
    }
    fs::create_dir_all(staging)?;
    Ok(())
}

/// Removes only a tree of ordinary files/directories, never traversing a
/// junction. Its parent must be protected from concurrent modification.
pub fn remove_directory(directory: &Path) -> Result<(), UpdateError> {
    reject_links(directory)?;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        reject_links(&entry.path())?;
        if entry.file_type()?.is_dir() {
            remove_directory(&entry.path())?;
        } else {
            fs::remove_file(entry.path())?;
        }
    }
    fs::remove_dir(directory)?;
    Ok(())
}

/// Revalidates a protected staging tree immediately before installation.
pub fn verify_directory(
    directory: &Path,
    installed: Version,
    public_key: &[u8; 32],
) -> Result<ReleaseManifest, UpdateError> {
    reject_links(directory)?;
    let manifest_bytes = read_limited(&directory.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
    let signature = read_limited(&directory.join(SIGNATURE_FILE), MAX_SIGNATURE_BYTES)?;
    let manifest = verified_newer(&manifest_bytes, &signature, installed, public_key)?;
    let mut total = 0_u64;
    for (relative, expected) in &manifest.files {
        let path =
            directory.join(safe_relative_path(relative).ok_or(UpdateError::InvalidManifest)?);
        reject_links(&path)?;
        let (actual, size) = hash_reader(
            &mut File::open(&path)?,
            &mut io::sink(),
            MAX_FILE_BYTES.min(MAX_TOTAL_BYTES.saturating_sub(total)),
        )?;
        total = total.saturating_add(size);
        if &actual != expected {
            return Err(UpdateError::HashMismatch(relative.clone()));
        }
    }
    Ok(manifest)
}

fn finish_staging(
    staging: &Path,
    manifest_bytes: &[u8],
    signature: &[u8],
    manifest: ReleaseManifest,
) -> Result<StagedUpdate, UpdateError> {
    fs::write(staging.join(MANIFEST_FILE), manifest_bytes)?;
    fs::write(staging.join(SIGNATURE_FILE), signature)?;
    Ok(StagedUpdate {
        manifest,
        directory: staging.to_path_buf(),
    })
}

/// Copies `input` to `target` while hashing it; on a mismatch or oversize
/// input the partial file is removed. Returns the number of bytes written.
fn write_verified(
    input: &mut impl Read,
    target: &Path,
    relative: &str,
    expected: &str,
    remaining_budget: u64,
) -> Result<u64, UpdateError> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(target)?;
    let result = hash_reader(input, &mut output, MAX_FILE_BYTES.min(remaining_budget)).and_then(
        |(hash, written)| {
            output.sync_all()?;
            if hash == expected {
                Ok(written)
            } else {
                Err(UpdateError::HashMismatch(relative.into()))
            }
        },
    );
    if result.is_err() {
        drop(output);
        let _ = fs::remove_file(target);
    }
    result
}

/// Streams `input` into `output`, returning the lowercase hex SHA-256 and the
/// byte count, and failing once more than `limit` bytes arrive.
fn hash_reader(
    input: &mut impl Read,
    output: &mut impl Write,
    limit: u64,
) -> Result<(String, u64), UpdateError> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut written = 0_u64;
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        written = written.saturating_add(count as u64);
        if written > limit {
            return Err(UpdateError::TooLarge);
        }
        hasher.update(&buffer[..count]);
        output.write_all(&buffer[..count])?;
    }
    Ok((encode_hex(&hasher.finalize()), written))
}

fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, UpdateError> {
    reject_links(path)?;
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| UpdateError::MissingFile(path.display().to_string()))?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(UpdateError::TooLarge);
    }
    Ok(bytes)
}

fn read_zip_entry(
    zip: &mut zip::ZipArchive<File>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>, UpdateError> {
    let entry = zip
        .by_name(&format!("{BUNDLE_PREFIX}{name}"))
        .map_err(|_| UpdateError::MissingFile(name.into()))?;
    let mut bytes = Vec::new();
    entry.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(UpdateError::TooLarge);
    }
    Ok(bytes)
}

/// A fixed repository that publishes signed Karma releases. Gitee mirrors
/// GitHub releases and assets, so either serves the same signed bundle; the
/// Ed25519 manifest, not the host, authenticates what is installed.
/// 发布签名版本的固定仓库。Gitee 同步 GitHub 的发布与附件，两者提供同一签名包；
/// 安装内容由 Ed25519 清单而非下载来源认证。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseSource {
    GitHub,
    Gitee,
}

impl ReleaseSource {
    /// Every source, in the order downloads are attempted.
    pub const ALL: [Self; 2] = [Self::GitHub, Self::Gitee];

    pub fn releases_url(self) -> &'static str {
        match self {
            Self::GitHub => "https://api.github.com/repos/csbuddhist/Karma/releases?per_page=30",
            Self::Gitee => {
                "https://gitee.com/api/v5/repos/karmakarma/Karma/releases?page=1&per_page=30&direction=desc"
            }
        }
    }

    fn download_prefix(self) -> &'static str {
        match self {
            Self::GitHub => "https://github.com/csbuddhist/Karma/releases/download/",
            Self::Gitee => "https://gitee.com/karmakarma/Karma/releases/download/",
        }
    }

    fn bundle_url(self, version: Version) -> String {
        format!(
            "{}v{version}/Karma-windows-x64-test-v{version}.zip",
            self.download_prefix()
        )
    }
}

/// A downloadable release newer than the installed version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub version: Version,
    pub source: ReleaseSource,
    pub download_url: String,
    /// Exact byte count when the source announces it. Gitee's release API
    /// omits sizes, so those downloads are only bounded by the global cap.
    pub size: Option<u64>,
}

/// The release fields GitHub and Gitee share.
#[derive(Deserialize)]
struct PublishedRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<PublishedAsset>,
}

#[derive(Deserialize)]
struct PublishedAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: Option<u64>,
}

/// Picks the newest non-draft release (prereleases included) that is newer
/// than `installed` and carries the Windows bundle ZIP from `source`.
pub fn select_release(
    source: ReleaseSource,
    releases_json: &[u8],
    installed: Version,
) -> Result<Option<ReleaseAsset>, UpdateError> {
    let releases: Vec<PublishedRelease> =
        serde_json::from_slice(releases_json).map_err(|_| UpdateError::Network)?;
    Ok(releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let version: Version = release.tag_name.strip_prefix('v')?.parse().ok()?;
            let expected_name = format!("Karma-windows-x64-test-v{version}.zip");
            release
                .assets
                .into_iter()
                .find(|asset| {
                    asset.name == expected_name
                        && asset.browser_download_url == source.bundle_url(version)
                        && asset
                            .size
                            .is_none_or(|size| size > 0 && size <= MAX_DOWNLOAD_BYTES)
                })
                .map(|asset| ReleaseAsset {
                    version,
                    source,
                    download_url: asset.browser_download_url,
                    size: asset.size,
                })
        })
        .filter(|asset| asset.version > installed)
        .max_by_key(|asset| asset.version))
}

fn http_agent(installed: Version) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout(HTTP_TIMEOUT * 10)
        .user_agent(&format!("Karma-Updater/{installed}"))
        .build()
}

/// Lists releases through the source's REST API.
pub fn fetch_releases(source: ReleaseSource, installed: Version) -> Result<Vec<u8>, UpdateError> {
    let mut request = http_agent(installed)
        .get(source.releases_url())
        .timeout(HTTP_TIMEOUT);
    if source == ReleaseSource::GitHub {
        request = request.set("Accept", "application/vnd.github+json");
    }
    let response = request.call().map_err(|_| UpdateError::Network)?;
    let mut body = Vec::new();
    response
        .into_reader()
        .take(MAX_RELEASES_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|_| UpdateError::Network)?;
    if body.len() as u64 > MAX_RELEASES_BYTES {
        return Err(UpdateError::TooLarge);
    }
    Ok(body)
}

/// Asks every source for releases and returns each source's newest bundle
/// newer than `installed`, newest version first and then in source order, so
/// a caller can fall back to a mirror holding the same version. Fails only
/// when no source answered.
/// 查询所有来源，返回各来源中比已安装版本新的最新安装包：版本高者在前，同版本按来源
/// 顺序排列，便于下载失败时改用镜像；只有所有来源都无响应时才返回错误。
pub fn find_updates(installed: Version) -> Result<Vec<ReleaseAsset>, UpdateError> {
    let mut answered = false;
    let mut found = Vec::new();
    for source in ReleaseSource::ALL {
        let selected = fetch_releases(source, installed)
            .and_then(|releases| select_release(source, &releases, installed));
        if let Ok(asset) = selected {
            answered = true;
            found.extend(asset);
        }
    }
    if !answered {
        return Err(UpdateError::Network);
    }
    order_updates(&mut found);
    Ok(found)
}

fn order_updates(assets: &mut [ReleaseAsset]) {
    let rank = |source: ReleaseSource| ReleaseSource::ALL.iter().position(|item| *item == source);
    assets.sort_by(|left, right| {
        right
            .version
            .cmp(&left.version)
            .then_with(|| rank(left.source).cmp(&rank(right.source)))
    });
}

/// Downloads `asset` to `destination`, rejecting more bytes than announced
/// or, when no size is announced, more than the download cap.
pub fn download(
    asset: &ReleaseAsset,
    destination: &Path,
    installed: Version,
) -> Result<(), UpdateError> {
    if asset.download_url != asset.source.bundle_url(asset.version)
        || asset
            .size
            .is_some_and(|size| size == 0 || size > MAX_DOWNLOAD_BYTES)
        || asset.version <= installed
    {
        return Err(UpdateError::Network);
    }
    let limit = asset.size.unwrap_or(MAX_DOWNLOAD_BYTES);
    reject_links(destination)?;
    let response = http_agent(installed)
        .get(&asset.download_url)
        .call()
        .map_err(|_| UpdateError::Network)?;
    let mut output = File::create(destination)?;
    let result =
        hash_reader(&mut response.into_reader(), &mut output, limit).and_then(|(_, written)| {
            output.sync_all()?;
            if asset.size.is_none_or(|size| written == size) && written > 0 {
                Ok(())
            } else {
                Err(UpdateError::Network)
            }
        });
    if result.is_err() {
        drop(output);
        let _ = fs::remove_file(destination);
    }
    result
}

pub fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(DIGITS[usize::from(byte >> 4)] as char);
        text.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    text
}

pub fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(text.get(index..index + 2)?, 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    use super::*;

    struct Fixture {
        _directory: tempfile::TempDir,
        bundle: PathBuf,
        staging: PathBuf,
        key: SigningKey,
    }

    impl Fixture {
        fn public_key(&self) -> [u8; 32] {
            self.key.verifying_key().to_bytes()
        }

        /// Signs the bundle as `version`.
        fn sign(&self, version: &str) {
            let manifest = build_manifest(&self.bundle, version.parse().unwrap()).unwrap();
            let bytes = serde_json::to_vec_pretty(&manifest).unwrap();
            fs::write(self.bundle.join(MANIFEST_FILE), &bytes).unwrap();
            fs::write(
                self.bundle.join(SIGNATURE_FILE),
                sign_manifest(&bytes, &self.key),
            )
            .unwrap();
        }

        fn zip(&self) -> PathBuf {
            let path = self.staging.with_extension("zip");
            let mut writer = zip::ZipWriter::new(File::create(&path).unwrap());
            let mut names = Vec::new();
            collect_all(&self.bundle, &self.bundle, &mut names);
            for relative in names {
                writer
                    .start_file(
                        format!("{BUNDLE_PREFIX}{relative}"),
                        zip::write::SimpleFileOptions::default(),
                    )
                    .unwrap();
                writer
                    .write_all(&fs::read(self.bundle.join(&relative)).unwrap())
                    .unwrap();
            }
            writer.finish().unwrap();
            path
        }
    }

    fn collect_all(root: &Path, directory: &Path, names: &mut Vec<String>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect_all(root, &path, names);
            } else {
                names.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    fn fixture() -> Fixture {
        let directory = tempfile::tempdir().unwrap();
        let canonical = directory.path().canonicalize().unwrap();
        let bundle = canonical.join("bundle");
        fs::create_dir_all(bundle.join("models/image")).unwrap();
        fs::write(bundle.join("KarmaService.exe"), b"service-binary").unwrap();
        fs::write(bundle.join("models/image/model.onnx"), b"model-bytes").unwrap();
        fs::write(bundle.join(".DS_Store"), b"finder").unwrap();
        Fixture {
            staging: canonical.join("staging"),
            _directory: directory,
            bundle,
            key: SigningKey::generate(&mut OsRng),
        }
    }

    const INSTALLED: Version = Version {
        major: 0,
        minor: 2,
        patch: 3,
    };

    #[test]
    fn versions_parse_and_order_numerically() {
        let parse = |text: &str| text.parse::<Version>().unwrap();
        assert!(parse("0.2.10") > parse("0.2.9"));
        assert!(parse("1.0.0") > parse("0.99.99"));
        assert_eq!(parse("0.2.4").to_string(), "0.2.4");
        for invalid in ["", "0.2", "0.2.4.1", "0.2.x", "v0.2.4", "0.2.-1", "0..4"] {
            assert!(invalid.parse::<Version>().is_err(), "{invalid}");
        }
    }

    #[test]
    fn manifest_paths_must_stay_inside_the_bundle() {
        assert_eq!(
            safe_relative_path("models/image/model.onnx"),
            Some(["models", "image", "model.onnx"].iter().collect())
        );
        for unsafe_path in [
            "",
            "/etc/passwd",
            "../outside",
            "a/../b",
            "a//b",
            "C:/Windows/x.exe",
            "a\\b",
            "con.",
            "CON.txt",
            "lpt1.dat",
            "COM1",
            "aux",
            "conout$",
            "a/.karma-update-backup/smuggle.exe",
            "COM\u{00b9}.exe",
            "bad\u{0}name",
        ] {
            assert_eq!(safe_relative_path(unsafe_path), None, "{unsafe_path:?}");
        }
    }

    #[test]
    fn signed_directory_is_staged_with_verified_copies() {
        let fixture = fixture();
        fixture.sign("0.2.4");
        let staged = stage_directory(
            &fixture.bundle,
            &fixture.staging,
            INSTALLED,
            &fixture.public_key(),
        )
        .unwrap();
        assert_eq!(staged.manifest.version(), "0.2.4".parse().unwrap());
        assert_eq!(
            staged.manifest.files.keys().collect::<Vec<_>>(),
            vec!["KarmaService.exe", "models/image/model.onnx"]
        );
        assert_eq!(
            fs::read(fixture.staging.join("models/image/model.onnx")).unwrap(),
            b"model-bytes"
        );
        assert!(fixture.staging.join(MANIFEST_FILE).exists());
        assert!(fixture.staging.join(SIGNATURE_FILE).exists());
        assert!(!fixture.staging.join(".DS_Store").exists());
    }

    #[test]
    fn signed_zip_is_staged_and_unlisted_entries_are_ignored() {
        let fixture = fixture();
        fixture.sign("0.2.4");
        fs::write(fixture.bundle.join("unlisted.exe"), b"smuggled").unwrap();
        let archive = fixture.zip();
        stage_zip(&archive, &fixture.staging, INSTALLED, &fixture.public_key()).unwrap();
        assert_eq!(
            fs::read(fixture.staging.join("KarmaService.exe")).unwrap(),
            b"service-binary"
        );
        assert!(!fixture.staging.join("unlisted.exe").exists());
    }

    #[test]
    fn forged_tampered_or_old_bundles_are_rejected() {
        let fixture = fixture();
        fixture.sign("0.2.4");
        let other_key = SigningKey::generate(&mut OsRng).verifying_key().to_bytes();
        assert!(matches!(
            stage_directory(&fixture.bundle, &fixture.staging, INSTALLED, &other_key),
            Err(UpdateError::BadSignature)
        ));
        // A file changed after signing fails its hash and is not left behind.
        fs::write(fixture.bundle.join("KarmaService.exe"), b"tampered").unwrap();
        assert!(matches!(
            stage_directory(&fixture.bundle, &fixture.staging, INSTALLED, &fixture.public_key()),
            Err(UpdateError::HashMismatch(path)) if path == "KarmaService.exe"
        ));
        assert!(!fixture.staging.join("KarmaService.exe").exists());
        // Editing the signed manifest breaks the signature.
        fixture.sign("0.2.4");
        let mut manifest = fs::read(fixture.bundle.join(MANIFEST_FILE)).unwrap();
        manifest.extend_from_slice(b" ");
        fs::write(fixture.bundle.join(MANIFEST_FILE), manifest).unwrap();
        assert!(matches!(
            stage_directory(
                &fixture.bundle,
                &fixture.staging,
                INSTALLED,
                &fixture.public_key()
            ),
            Err(UpdateError::BadSignature)
        ));
        // Reinstalling or downgrading is refused even when correctly signed.
        for version in ["0.2.3", "0.2.2"] {
            fixture.sign(version);
            assert!(matches!(
                stage_directory(
                    &fixture.bundle,
                    &fixture.staging,
                    INSTALLED,
                    &fixture.public_key()
                ),
                Err(UpdateError::NotNewer { .. })
            ));
        }
    }

    #[test]
    fn manifests_listing_unsafe_paths_are_rejected_even_when_signed() {
        let fixture = fixture();
        let manifest = ReleaseManifest {
            format: MANIFEST_FORMAT,
            product: PRODUCT.into(),
            version: "0.2.4".into(),
            files: BTreeMap::from([("../escape.exe".into(), "0".repeat(64))]),
        };
        let bytes = serde_json::to_vec(&manifest).unwrap();
        assert!(matches!(
            verify_manifest(
                &bytes,
                &sign_manifest(&bytes, &fixture.key),
                &fixture.public_key()
            ),
            Err(UpdateError::InvalidManifest)
        ));
    }

    #[test]
    fn signed_case_aliases_and_file_directory_conflicts_are_rejected() {
        let fixture = fixture();
        for paths in [
            vec!["KarmaService.exe", "karmaservice.exe"],
            vec!["models", "models/image.onnx"],
            vec!["MODELS", "models/image.onnx"],
            vec!["RELEASE-MANIFEST.JSON"],
        ] {
            let manifest = ReleaseManifest {
                format: 1,
                product: PRODUCT.into(),
                version: "0.2.4".into(),
                files: paths
                    .into_iter()
                    .map(|path| (path.into(), "0".repeat(64)))
                    .collect(),
            };
            let bytes = serde_json::to_vec(&manifest).unwrap();
            assert!(matches!(
                verify_manifest(
                    &bytes,
                    &sign_manifest(&bytes, &fixture.key),
                    &fixture.public_key()
                ),
                Err(UpdateError::InvalidManifest)
            ));
        }
    }

    #[test]
    fn prepared_copy_is_rehashed_before_installation() {
        let fixture = fixture();
        fixture.sign("0.2.4");
        stage_directory(
            &fixture.bundle,
            &fixture.staging,
            INSTALLED,
            &fixture.public_key(),
        )
        .unwrap();
        verify_directory(&fixture.staging, INSTALLED, &fixture.public_key()).unwrap();
        fs::write(
            fixture.staging.join("KarmaService.exe"),
            b"changed-after-copy",
        )
        .unwrap();
        assert!(matches!(
            verify_directory(&fixture.staging, INSTALLED, &fixture.public_key()),
            Err(UpdateError::HashMismatch(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn source_symlinks_and_destination_symlinks_are_rejected_without_traversal() {
        use std::os::unix::fs::symlink;
        let fixture = fixture();
        fixture.sign("0.2.4");
        let outside = fixture.bundle.parent().unwrap().join("outside.exe");
        fs::write(&outside, b"service-binary").unwrap();
        fs::remove_file(fixture.bundle.join("KarmaService.exe")).unwrap();
        symlink(&outside, fixture.bundle.join("KarmaService.exe")).unwrap();
        assert!(matches!(
            stage_directory(
                &fixture.bundle,
                &fixture.staging,
                INSTALLED,
                &fixture.public_key()
            ),
            Err(UpdateError::UnsafePath)
        ));
        fs::remove_file(fixture.bundle.join("KarmaService.exe")).unwrap();
        fs::copy(&outside, fixture.bundle.join("KarmaService.exe")).unwrap();
        if fixture.staging.exists() {
            remove_directory(&fixture.staging).unwrap();
        }
        symlink(&fixture.bundle, &fixture.staging).unwrap();
        assert!(matches!(
            stage_directory(
                &fixture.bundle,
                &fixture.staging,
                INSTALLED,
                &fixture.public_key()
            ),
            Err(UpdateError::UnsafePath)
        ));
        assert_eq!(fs::read(&outside).unwrap(), b"service-binary");
        assert!(fixture.bundle.join(MANIFEST_FILE).exists());
    }

    #[cfg(unix)]
    #[test]
    fn hardlinked_source_is_rejected_and_external_file_is_unchanged() {
        let fixture = fixture();
        fixture.sign("0.2.4");
        let external = fixture.bundle.parent().unwrap().join("external.exe");
        fs::hard_link(fixture.bundle.join("KarmaService.exe"), &external).unwrap();
        assert!(matches!(
            stage_directory(
                &fixture.bundle,
                &fixture.staging,
                INSTALLED,
                &fixture.public_key()
            ),
            Err(UpdateError::UnsafePath)
        ));
        assert_eq!(fs::read(&external).unwrap(), b"service-binary");
    }

    #[test]
    fn oversized_manifest_and_noncanonical_asset_url_fail_before_io() {
        assert!(matches!(
            verify_manifest(
                &vec![b' '; MAX_MANIFEST_BYTES as usize + 1],
                "",
                &RELEASE_PUBLIC_KEY
            ),
            Err(UpdateError::TooLarge)
        ));
        let asset = ReleaseAsset {
            version: "0.2.4".parse().unwrap(),
            source: ReleaseSource::GitHub,
            download_url: "https://github.com/csbuddhist/Karma/releases/download/other/evil.zip"
                .into(),
            size: Some(10),
        };
        assert!(matches!(
            download(&asset, Path::new("must-not-be-created.zip"), INSTALLED),
            Err(UpdateError::Network)
        ));
        // A URL of the other repository never passes for this source.
        let crossed = ReleaseAsset {
            source: ReleaseSource::Gitee,
            download_url: ReleaseSource::GitHub.bundle_url(asset.version),
            size: None,
            ..asset
        };
        assert!(matches!(
            download(&crossed, Path::new("must-not-be-created.zip"), INSTALLED),
            Err(UpdateError::Network)
        ));
    }

    #[test]
    fn newest_matching_release_is_selected() {
        let releases = serde_json::to_vec(&serde_json::json!([
            {"tag_name": "v0.2.5", "draft": true, "assets": [
                {"name": "Karma-windows-x64-test-v0.2.5.zip", "size": 10,
                 "browser_download_url": "https://github.com/csbuddhist/Karma/releases/download/v0.2.5/Karma-windows-x64-test-v0.2.5.zip"}
            ]},
            {"tag_name": "v0.2.4", "prerelease": true, "assets": [
                {"name": "Karma-windows-x64-test-v0.2.4-setup.exe", "size": 10,
                 "browser_download_url": "https://github.com/csbuddhist/Karma/releases/download/v0.2.4/Karma-windows-x64-test-v0.2.4-setup.exe"},
                {"name": "Karma-windows-x64-test-v0.2.4.zip", "size": 42,
                 "browser_download_url": "https://github.com/csbuddhist/Karma/releases/download/v0.2.4/Karma-windows-x64-test-v0.2.4.zip"}
            ]},
            {"tag_name": "v0.2.3", "assets": [
                {"name": "Karma-windows-x64-test-v0.2.3.zip", "size": 10,
                 "browser_download_url": "https://github.com/csbuddhist/Karma/releases/download/v0.2.3/Karma-windows-x64-test-v0.2.3.zip"}
            ]},
            {"tag_name": "v9.9.9", "assets": [
                {"name": "Karma-windows-x64-test-v9.9.9.zip", "size": 10,
                 "browser_download_url": "https://evil.example/Karma-windows-x64-test-v9.9.9.zip"}
            ]}
        ]))
        .unwrap();
        assert_eq!(
            select_release(ReleaseSource::GitHub, &releases, INSTALLED).unwrap(),
            Some(ReleaseAsset {
                version: "0.2.4".parse().unwrap(),
                source: ReleaseSource::GitHub,
                download_url: "https://github.com/csbuddhist/Karma/releases/download/v0.2.4/Karma-windows-x64-test-v0.2.4.zip".into(),
                size: Some(42),
            })
        );
        assert_eq!(
            select_release(ReleaseSource::GitHub, &releases, "0.2.4".parse().unwrap()).unwrap(),
            None
        );
        // GitHub download URLs are not Gitee assets.
        assert_eq!(
            select_release(ReleaseSource::Gitee, &releases, INSTALLED).unwrap(),
            None
        );
    }

    #[test]
    fn gitee_releases_without_sizes_are_selected_from_the_mirror_only() {
        let releases = serde_json::to_vec(&serde_json::json!([
            {"tag_name": "v0.2.6", "prerelease": true, "assets": [
                {"name": "Karma-windows-x64-test-v0.2.6.zip",
                 "browser_download_url": "https://gitee.com/someone/Karma/releases/download/v0.2.6/Karma-windows-x64-test-v0.2.6.zip"}
            ]},
            {"tag_name": "v0.2.5", "prerelease": true, "assets": [
                {"name": "Karma-windows-x64-test-v0.2.5-setup.exe",
                 "browser_download_url": "https://gitee.com/karmakarma/Karma/releases/download/v0.2.5/Karma-windows-x64-test-v0.2.5-setup.exe"},
                {"name": "Karma-windows-x64-test-v0.2.5.zip",
                 "browser_download_url": "https://gitee.com/karmakarma/Karma/releases/download/v0.2.5/Karma-windows-x64-test-v0.2.5.zip"},
                {"name": "v0.2.5.zip",
                 "browser_download_url": "https://gitee.com/karmakarma/Karma/archive/refs/tags/v0.2.5.zip"}
            ]}
        ]))
        .unwrap();
        assert_eq!(
            select_release(ReleaseSource::Gitee, &releases, INSTALLED).unwrap(),
            Some(ReleaseAsset {
                version: "0.2.5".parse().unwrap(),
                source: ReleaseSource::Gitee,
                download_url: "https://gitee.com/karmakarma/Karma/releases/download/v0.2.5/Karma-windows-x64-test-v0.2.5.zip".into(),
                size: None,
            })
        );
        assert_eq!(
            select_release(ReleaseSource::GitHub, &releases, INSTALLED).unwrap(),
            None
        );
    }

    #[test]
    fn newer_versions_come_first_and_github_precedes_its_mirror() {
        let asset = |version: &str, source: ReleaseSource| {
            let version: Version = version.parse().unwrap();
            ReleaseAsset {
                version,
                source,
                download_url: source.bundle_url(version),
                size: None,
            }
        };
        let mut found = vec![
            asset("0.2.5", ReleaseSource::Gitee),
            asset("0.2.4", ReleaseSource::GitHub),
            asset("0.2.5", ReleaseSource::GitHub),
        ];
        order_updates(&mut found);
        assert_eq!(
            found,
            vec![
                asset("0.2.5", ReleaseSource::GitHub),
                asset("0.2.5", ReleaseSource::Gitee),
                asset("0.2.4", ReleaseSource::GitHub),
            ]
        );
    }

    #[test]
    fn hex_round_trips() {
        assert_eq!(encode_hex(&[0x00, 0xab, 0xff]), "00abff");
        assert_eq!(decode_hex("00abff"), Some(vec![0x00, 0xab, 0xff]));
        assert_eq!(decode_hex("0"), None);
        assert_eq!(decode_hex("zz"), None);
    }
}
