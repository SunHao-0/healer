#ifndef HEALER_UNIX_SOCK
#define HEALER_UNIX_SOCK

#if GOOS_linux
#include <dirent.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define PORT_STDIN 30
#define PORT_STDOUT 29
#define PORT_STDERR 28

int open_vport_dev(int id, int port)
{
	char buf[FILENAME_MAX];
	int fd;

	snprintf(buf, FILENAME_MAX, "/dev/vport%dp%d", id, port);
	fd = open(buf, O_RDWR);
	if (fd < 0) {
		failmsg("failed to open: ", "%s", buf);
	}
	return fd;
}

void setup_unix_sock()
{
	int mappings[3][2] = {{PORT_STDIN, 0}, {PORT_STDOUT, 1}, {PORT_STDERR, 2}};
	int i = 0;
	int fd;

	for (; i != 3; i++) {
		fd = open_vport_dev(i + 1, mappings[i][0]);
		if (dup2(fd, mappings[i][1]) < 0) {
			failmsg("failed to dup:", "%d -> %d", fd, mappings[i][1]);
		}
		close(fd);
	}
}

#define SETUP_UNIX_SOCKS_SNIPPET                                           \
	do {                                                               \
		if (argc >= 3 && strcmp(argv[2], "use-unix-socks") == 0) { \
			setup_unix_sock();                                 \
		}                                                          \
	} while (0)

#else
#error Currently, ivshm_setup only supports linux.
#endif // GOOS_linux

#endif // HEALER_UNIX_SOCK