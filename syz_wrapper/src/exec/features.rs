use super::*;
use iota::iota;
use thiserror::Error;

pub type Features = u64;

iota! {
    pub const FEATURE_COVERAGE: Features = 1 << (iota);
    ,FEATURE_COMPARISONS
    ,FEATURE_EXTRA_COVERAGE
    ,FEATURE_SANDBOX_SETUID
    ,FEATURE_SANDBOX_NAMESPACE
    ,FEATURE_SANDBOX_ANDROID
    ,FEATURE_FAULT
    ,FEATURE_LEAK
    ,FEATURE_NET_INJECTION
    ,FEATURE_NET_DEVICES
    ,FEATURE_KCSAN
    ,FEATURE_DEVLINK_PCI
    ,FEATURE_USB_EMULATION
    ,FEATURE_VHCI_INJECTION
    ,FEATURE_WIFI_EMULATION
    ,FEATURE_802154
}

pub const FEATURES_NAME: [&str; 16] = [
    "code coverage",
    "comparison tracing",
    "extra coverage",
    "setuid sandbox",
    "namespace sandbox",
    "Android sandbox",
    "fault injection",
    "leak checking",
    "net packet injection",
    "net device setup",
    "concurrency sanitizer",
    "devlink PCI setup",
    "USB emulation",
    "hci packet injection",
    "wifi device emulation",
    "802.15.4 emulation",
];

#[derive(Debug, Error)]
pub enum DetectFeaturesError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("detect: {0}")]
    Detect(String),
}

pub fn detect_features(mut cmd: Command) -> Result<Features, DetectFeaturesError> {
    cmd.arg("check");
    let output = cmd.output()?;
    if output.status.success() {
        let out = output.stdout;
        assert_eq!(out.len(), 8);
        let mut val = [0; 8];
        val.copy_from_slice(&out[0..]);
        Ok(u64::from_le_bytes(val))
    } else {
        let err = String::from_utf8_lossy(&output.stderr).into_owned();
        Err(DetectFeaturesError::Detect(format!(
            "'{:?}' : {}",
            cmd, err
        )))
    }
}

#[derive(Debug, Error)]
pub enum SetupFeaturesError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("setup: {0}")]
    Setup(String),
}

pub fn setup_features(mut cmd: Command, features: Features) -> Result<(), SetupFeaturesError> {
    let feature_args = features_to_args(features);
    if feature_args.is_empty() {
        return Ok(());
    }

    cmd.arg("setup").args(&feature_args);
    let output = cmd.output()?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr).into_owned();
        return Err(SetupFeaturesError::Setup(format!(
            "failed to run '{:?}': {}",
            cmd, err
        )));
    }

    Ok(())
}

fn features_to_args(features: Features) -> Vec<String> {
    let mut ret = Vec::new();

    if features & FEATURE_LEAK != 0 {
        ret.push("leak".to_string());
    }
    if features & FEATURE_FAULT != 0 {
        ret.push("fault".to_string());
    }
    if features & FEATURE_KCSAN != 0 {
        ret.push("kcsan".to_string());
    }
    if features & FEATURE_USB_EMULATION != 0 {
        ret.push("usb".to_string());
    }
    if features & FEATURE_802154 != 0 {
        ret.push("802154".to_string());
    }

    ret
}

pub fn features_to_env_flags(features: Features, env: &mut EnvFlags) {
    if features & FEATURE_EXTRA_COVERAGE != 0 {
        *env |= FLAG_EXTRA_COVER;
    }
    if features & FEATURE_NET_INJECTION != 0 {
        *env |= FLAG_ENABLE_TUN;
    }
    if features & FEATURE_NET_DEVICES != 0 {
        *env |= FLAG_ENABLE_NETDEV;
    }

    *env |= FLAG_ENABLE_NETRESET;
    *env |= FLAG_ENABLE_CGROUPS;
    *env |= FLAG_ENABLE_CLOSEFDS;

    if features & FEATURE_DEVLINK_PCI != 0 {
        *env |= FLAG_ENABLE_DEVLINKPCI;
    }
    if features & FEATURE_VHCI_INJECTION != 0 {
        *env |= FLAG_ENABLE_VHCI_INJECTION;
    }
    if features & FEATURE_WIFI_EMULATION != 0 {
        *env |= FLAG_ENABLE_WIFI;
    }
}
