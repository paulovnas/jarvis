//! App-owned patched engines are bound to the exact source, patch and host.
//! Legacy independently downloaded Core packs keep their release receipts.
use super::{invalid, CoreError, CLI_VERSION, ENGINE_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SOURCE_COMMIT: &str = "d631a8827f99414d2b6daba4ef08b7f8701751d7";
const PATCH_PATH: &str = "scripts/impeccable-native/queue-cas.patch";
const PATCH_BYTES: &[u8] = include_bytes!("../../../../scripts/impeccable-native/queue-cas.patch");
const DESIGN_PATCH_PATH: &str = "scripts/impeccable-native/canonical-design.patch";
const DESIGN_PATCH_BYTES: &[u8] =
    include_bytes!("../../../../scripts/impeccable-native/canonical-design.patch");
const PRODUCT_PATCH_PATH: &str = "scripts/impeccable-native/canonical-product.patch";
const PRODUCT_PATCH_BYTES: &[u8] =
    include_bytes!("../../../../scripts/impeccable-native/canonical-product.patch");
const ACCEPT_PATCH_PATH: &str = "scripts/impeccable-native/accept-write.patch";
const ACCEPT_PATCH_BYTES: &[u8] =
    include_bytes!("../../../../scripts/impeccable-native/accept-write.patch");
const LIVE_DESIGN_PATCH_PATH: &str = "scripts/impeccable-native/live-design-refresh.patch";
const LIVE_DESIGN_PATCH_BYTES: &[u8] =
    include_bytes!("../../../../scripts/impeccable-native/live-design-refresh.patch");

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Patch {
    path: String,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Receipt {
    schema_version: u32,
    engine_version: String,
    cli_version: String,
    source_commit: String,
    target: String,
    patches: Vec<Patch>,
    engine_sha256: String,
}

fn target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("darwin-arm64"),
        ("macos", "x86_64") => Some("darwin-x64"),
        ("linux", "aarch64") => Some("linux-arm64"),
        ("linux", "x86_64") => Some("linux-x64"),
        ("windows", "x86_64") => Some("windows-x64"),
        _ => None,
    }
}

fn patches() -> Vec<Patch> {
    vec![
        Patch {
            path: PATCH_PATH.into(),
            sha256: format!("{:x}", Sha256::digest(PATCH_BYTES)),
        },
        Patch {
            path: DESIGN_PATCH_PATH.into(),
            sha256: format!("{:x}", Sha256::digest(DESIGN_PATCH_BYTES)),
        },
        Patch {
            path: PRODUCT_PATCH_PATH.into(),
            sha256: format!("{:x}", Sha256::digest(PRODUCT_PATCH_BYTES)),
        },
        Patch {
            path: ACCEPT_PATCH_PATH.into(),
            sha256: format!("{:x}", Sha256::digest(ACCEPT_PATCH_BYTES)),
        },
        Patch {
            path: LIVE_DESIGN_PATCH_PATH.into(),
            sha256: format!("{:x}", Sha256::digest(LIVE_DESIGN_PATCH_BYTES)),
        },
    ]
}

pub(super) fn verify(receipt: &Receipt, engine_sha256: &str) -> Result<(), CoreError> {
    if receipt.schema_version != 1
        || receipt.engine_version != ENGINE_VERSION
        || receipt.cli_version != CLI_VERSION
        || receipt.source_commit != SOURCE_COMMIT
        || target() != Some(receipt.target.as_str())
        || receipt.patches != patches()
        || receipt.engine_sha256 != engine_sha256
        || engine_sha256.len() != 64
        || !engine_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> Receipt {
        Receipt {
            schema_version: 1,
            engine_version: ENGINE_VERSION.into(),
            cli_version: CLI_VERSION.into(),
            source_commit: SOURCE_COMMIT.into(),
            target: target().unwrap().into(),
            patches: patches(),
            engine_sha256: "a".repeat(64),
        }
    }

    #[test]
    fn accepts_only_the_native_engine_built_for_this_host_and_patch() {
        let original = receipt();
        assert!(verify(&original, &original.engine_sha256).is_ok());
        let mut changed = original.clone();
        changed.source_commit = "b".repeat(40);
        assert!(verify(&changed, &original.engine_sha256).is_err());
        changed = original.clone();
        changed.patches[0].sha256 = "b".repeat(64);
        assert!(verify(&changed, &original.engine_sha256).is_err());
        changed = original.clone();
        changed.target = "other-host".into();
        assert!(verify(&changed, &original.engine_sha256).is_err());
        changed = original.clone();
        changed.engine_version = format!("{ENGINE_VERSION}-untrusted");
        assert!(verify(&changed, &original.engine_sha256).is_err());
        assert!(verify(&original, &"b".repeat(64)).is_err());
    }

    #[test]
    fn native_product_routing_requires_the_exact_patch_and_application_order() {
        let original = receipt();
        assert_eq!(
            original
                .patches
                .iter()
                .map(|patch| patch.path.as_str())
                .collect::<Vec<_>>(),
            [
                PATCH_PATH,
                DESIGN_PATCH_PATH,
                PRODUCT_PATCH_PATH,
                ACCEPT_PATCH_PATH,
                "scripts/impeccable-native/live-design-refresh.patch"
            ]
        );
        let mut changed = original.clone();
        changed.patches.remove(2);
        assert!(verify(&changed, &original.engine_sha256).is_err());
        changed = original.clone();
        changed.patches.swap(2, 3);
        assert!(verify(&changed, &original.engine_sha256).is_err());
        changed = original.clone();
        changed.patches[2].sha256 = "b".repeat(64);
        assert!(verify(&changed, &original.engine_sha256).is_err());
        changed = original.clone();
        changed.patches.remove(4);
        assert!(verify(&changed, &original.engine_sha256).is_err());
        changed = original.clone();
        changed.patches[4].sha256 = "b".repeat(64);
        assert!(verify(&changed, &original.engine_sha256).is_err());
    }

    #[test]
    fn native_bundle_cannot_fall_back_to_an_unpatched_core_release() {
        let resources = tempfile::tempdir().unwrap();
        let directory = resources.path().join("impeccable");
        std::fs::create_dir(&directory).unwrap();
        super::super::tests::prepare_fixture(&directory, &[]).unwrap();
        // Existing Core fixtures are still readable for legacy recovery.
        assert!(super::super::Pack::at(&directory, "1.2.3").is_ok());
        let home = tempfile::tempdir().unwrap();
        // Application setup requires native provenance before touching OnceLock.
        assert!(super::super::setup(resources.path(), home.path()).is_err());
        let index_path = directory.join("jarvis-design.json");
        let mut index: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&index_path).unwrap()).unwrap();
        let mut valid = receipt();
        valid.engine_sha256 = index["engine_sha256"].as_str().unwrap().into();
        index["native_engine"] = serde_json::to_value(&valid).unwrap();
        std::fs::write(&index_path, serde_json::to_vec(&index).unwrap()).unwrap();
        assert!(super::super::Pack::at(&directory, "1.2.3").is_ok());
        index["native_engine"]["patches"][0]["sha256"] = serde_json::json!("0".repeat(64));
        std::fs::write(&index_path, serde_json::to_vec(&index).unwrap()).unwrap();
        assert!(super::super::Pack::at(&directory, "1.2.3").is_err());
    }
}
