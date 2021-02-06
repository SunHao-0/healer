#ifndef FEATURES
#define FEATURES

#if GOOS_linux
#include <ctype.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#include <fcntl.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/utsname.h>
#include <unistd.h>

#ifdef __x86_64__
typedef unsigned long long u64;
#else
typedef uint64_t u64;
#endif

#define LINUX_FEATURES_CHECK_SNIPPET1                               \
	do {                                                            \
		if (argc == 2 && strcmp(argv[1], "check") == 0) {           \
			u64 features = to_le(check());                          \
			if (fwrite(&features, 1, 8, stdout) < 0) {              \
				fail("failed to write features to stdout");         \
			}                                                       \
			return 0;                                               \
		}                                                           \
	} while (0)

static inline void __swap(char* a, char* b)
{
	char tmp = *a;
	*a = *b;
	*b = tmp;
}

static u64 to_le(u64 n)
{
	int i = 1;
	if ((*(char*)(&i)) == 0) { // is bigendian?
		char* buf = (char*)&n;
		__swap(&buf[0], &buf[7]);
		__swap(&buf[1], &buf[6]);
		__swap(&buf[2], &buf[5]);
		__swap(&buf[3], &buf[4]);
	}
	return n;
}

static bool has_debugfs()
{
	return access("/sys/kernel/debug", F_OK) == 0;
}

static bool has_kcov()
{
	return has_debugfs() && access("/sys/kernel/debug/kcov", F_OK) == 0;
}

static bool has_fault()
{
	return access("/proc/self/make-it-fail", F_OK) &&
	       access("/proc/thread-self/fail-nth", F_OK) &&
	       has_debugfs() &&
	       access("/sys/kernel/debug/failslab/ignore-gfp-wait", F_OK);
}

static bool has_leak()
{
	if (!has_debugfs()) {
		return false;
	}
	int f;
	if ((f = open("/sys/kernel/debug/kmemleak", O_RDWR)) != 0) {
		return false;
	}
	if (write(f, "scan=off", 8) < 0) {
		return false;
	}
	close(f);
	return true;
}

static bool has_ns()
{
	return access("/proc/self/ns/user", F_OK) == 0;
}

static bool has_android()
{
	return access("/sys/fs/selinux/policy", F_OK) == 0;
}

static bool has_tun()
{
	return access("/dev/net/tun", F_OK) == 0;
}

static bool has_usb()
{
	return access("/dev/raw-gadget", F_OK) == 0;
}

static bool has_vhci()
{
	return access("/dev/vhci", F_OK) == 0;
}

static bool has_kcsan()
{
	return access("/sys/kernel/debug/kcsan", F_OK) == 0;
}

static bool has_devlink_pci()
{
	return access("/sys/bus/pci/devices/0000:00:10.0/", F_OK) == 0;
}

static bool check_kversion(int major, int minor)
{
	struct utsname buf;
	char* p;
	long ver[16];
	int i = 0;

	if (uname(&buf) != 0) {
		return false;
	}
	p = buf.release;
	while (*p && i < 4) {
		if (isdigit(*p)) {
			ver[i] = strtol(p, &p, 10);
			i++;
		} else {
			p++;
		}
	}

	return i >= 2 && ver[1] * 1000 + ver[2] >= major * 1000 + minor;
}

static bool has_wifi()
{
	return check_kversion(4, 17) && access("/sys/class/mac80211_hwsim/", F_OK) == 0;
}

static bool unused()
{
	return false;
}

static bool enable()
{
	return true;
}

static bool (*checkers[15])() = {
    has_kcov, // FEATURE_COVERAGE
    unused, // FEATURE_COMPARISONS
    unused, // FEATURE_EXTRA_COVERAGE
    enable, // FEATURE_SANDBOX_SETUID
    has_ns, // FEATURE_SANDBOX_NAMESPACE
    has_android, // FEATURE_SANDBOX_ANDROID
    has_fault, // FEATURE_FAULT
    has_leak, // FEATURE_LEAK
    has_tun, // FEATURE_NET_INJECTION
    enable, // FEATURE_NET_DEVICES
    has_kcsan, // FEATURE_KCSAN
    has_devlink_pci, // FEATURE_DEVLINK_PCI
    has_usb, // FEATURE_USB_EMULATION
    has_vhci, // FEATURE_VHCI_INJECTION
    has_wifi // FEATURE_WIFI_EMULATION
};

static u64 check()
{
	u64 ret = 0;
	int i;
	for (i = 0; i < 15; i++) {
		if (checkers[i]()) {
			ret |= (1 << i);
		}
	}

	return ret;
}
#else
#error Currently, features has only supports linux.
#endif

#endif // FEATURES