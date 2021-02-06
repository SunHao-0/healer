use iota::iota;

use crate::exec::ExecHandle;
iota! {
    pub const FEATURE_COVERAGE: u64 = 1 << (iota);
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
}

pub fn check(handle: &mut ExecHandle, verbose: bool) -> u64 {
    const FEATURES: [&str; 15] = [
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
    ];
    let features = handle
        .check_features()
        .unwrap_or(FEATURE_COVERAGE | FEATURE_SANDBOX_SETUID | FEATURE_NET_DEVICES);
    if verbose {
        for (i, feature) in FEATURES.iter().enumerate() {
            if features & (1 << i) == 1 {
                log::info!("{:<28}: enabled", feature);
            }
        }
    }
    features
}
