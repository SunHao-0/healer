#ifndef IVSHM_SETUP
#define IVSHM_SETUP

#if GOOS_linux
#include <dirent.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define IVSHMEM_PCI_VENDOR_ID 0x1af4
#define IVSHMEM_PCI_DEVICE_ID 0x1110
#define PCI_SYSFS_PATH "/sys/bus/pci/devices"
static int in_fd_inner = -1, out_fd_inner = -1;

static char* read_str(char* f)
{
	static char buf[256];
	int fd, n;
	fd = open(f, O_RDONLY);
	if (fd < 0) {
		return NULL;
	}
	n = read(fd, buf, 256);
	close(fd);
	if (n < 0 || n >= 256) {
		return NULL;
	}
	buf[n] = 0;
	return buf;
}

static long read_val(char* f)
{
	char* str;
	long val = -1;
	str = read_str(f);
	if (str) {
		val = strtol(str, NULL, 0);
	}
	return val;
}

static long get_resource2_sz(char* fname)
{
	char buf[256];
	FILE* f;
	unsigned long long start, end, size = -1, flags;
	if ((f = fopen(fname, "r"))) {
		// skip 0,1
		if (fgets(buf, 256, f) && fgets(buf, 256, f)) {
			if (fgets(buf, 256, f)) {
				if (sscanf(buf, "%llx %llx %llx", &start, &end, &flags) == 3) {
					if (end > start)
						size = end - start + 1;
				}
			}
		}
		fclose(f);
	}
	return size;
}

static void scan_pci_device()
{
	DIR* pci_dir;
	struct dirent* entry;
	char dir_name[FILENAME_MAX];

	pci_dir = opendir(PCI_SYSFS_PATH);
	if (!pci_dir) {
		fail("failed to open %s", PCI_SYSFS_PATH);
	}
	while ((entry = readdir(pci_dir))) {
		long vendor, device, ragion_sz;
		int fd;
		// skip ".", "..", or other special device.
		if (entry->d_name[0] == '.')
			continue;

		snprintf(dir_name, FILENAME_MAX, "%s/%s/vendor", PCI_SYSFS_PATH, entry->d_name);
		vendor = read_val(dir_name);
		snprintf(dir_name, FILENAME_MAX, "%s/%s/device", PCI_SYSFS_PATH, entry->d_name);
		device = read_val(dir_name);

		if (vendor == IVSHMEM_PCI_VENDOR_ID && device == IVSHMEM_PCI_DEVICE_ID) {
			snprintf(dir_name, FILENAME_MAX, "%s/%s/resource", PCI_SYSFS_PATH, entry->d_name);
			ragion_sz = get_resource2_sz(dir_name);
			snprintf(dir_name, FILENAME_MAX, "%s/%s/resource2", PCI_SYSFS_PATH, entry->d_name);
			fd = open(dir_name, O_RDWR);
			if (ragion_sz == kMaxOutput) {
				out_fd_inner = fd;
			} else if (ragion_sz == kMaxInput) {
				in_fd_inner = fd;
			} else {
				fail("unexpect ivshm size: %ld", ragion_sz);
			}
		}
	}
	closedir(pci_dir);
}

static void ivshm_setup(int in_fd, int out_fd)
{
	scan_pci_device();
	if (in_fd_inner == -1 || out_fd_inner == -1) {
		fail("failed to setup ivshm");
	}
	if (dup2(in_fd_inner, in_fd) < 0) {
		fail("failed to dup: %d -> %d.", in_fd_inner, in_fd);
	}
	if (dup2(out_fd_inner, out_fd) < 0) {
		fail("failed to dup: %d -> %d.", in_fd_inner, in_fd);
	}
}

#define IVSHM_SETUP_SNIPPET                                           \
	do {                                                          \
		if (argc == 2 && strcmp(argv[1], "use-ivshm") == 0) { \
			ivshm_setup(kInFd, kOutFd);                   \
		}                                                     \
	} while (0)

#else
#error Currently, ivshm_setup only supports linux.
#endif

#endif // IVSHM_SETUP