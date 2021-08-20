# Healer
[![Build](https://github.com/SunHao-0/healer/workflows/Build/badge.svg?branch=master)](https://github.com/SunHao-0/healer/actions?query=workflow%3ABuild) 

Healer is a kernel fuzzer inspired by [Syzkaller](https://github.com/google/syzkaller).

Similar to Syzkaller, Healer uses the syscall information provided by the [Syzlang](https://github.com/google/syzkaller/blob/master/docs/syscall_descriptions.md) [description](https://github.com/google/syzkaller/tree/master/sys/linux) to generate sequences of system calls that confirm to the parameter structure constraints and partial semantic constraints, and finds kernel bugs by continuously executing the generated call sequences to cause kernel crashes.

Unlike Syzkaller, Healer does not use an empirical [choice-table](https://github.com/google/syzkaller/blob/master/prog/prio.go), but detects the influence relationships between syscalls by dynamically removing calls in the minimized call sequences and observing coverage changes, and uses the influence relationships to guide the generation and mutation of call sequences. In addition, Healer also uses a different architectural design than Syzkaller.

When the [refactoring](https://github.com/SunHao-0/healer/tree/refactor) code is stable, we will add more information about how Healer works in the documentation.

## Build Healer

Healer is written in pure rust, except for some patching code. Therefore, [rust](https://www.rust-lang.org/) toolchain should be installed first.

``` shell 
> curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
> rustc --version # check install
```

In order to use the Syzlang descriptions, Healer's [build script](https://github.com/SunHao-0/healer/blob/master/build.rs) will automatically *download* Syzkaller, which may increase the build time, add [patches](https://github.com/SunHao-0/healer/tree/master/patches) to the source code and build Syzkaller. Therefore, the [build tool](https://github.com/google/syzkaller/blob/master/docs/linux/setup.md) required for Syzkaller needs to be installed, e.g. golang compiler with GO111MODULE on, GCC 6.1.0 or later.

Once all the required tools have been installed, Healer can be easily built using following command:

``` shell
> cargo build --release
```

Finally, Healer itself and the patched Syzkaller binary (`syz-bin`) can be found in the `target/release` directory.

## Fuzz Linux Kernel with Healer

Overall, fuzzing Linux kernel with Healer requires three steps: (1) prepare the disk image, (2) compile the kernel, and (3) start Healer. 

Healer uses qumu to boot the kernel, so the disk image and kernel image need to be prepared. The booted qemu needs to be able to log in via the ssh key, and the kernel needs to have at least the `kcov` feature. [This document](https://github.com/google/syzkaller/blob/master/docs/linux/setup_ubuntu-host_qemu-vm_x86-64-kernel.md) from Syzkaller describes in detail how to build `stretch.img` and compile the Linux kernel with specific configuration, so follow the instructions there to complete the first two steps.

Once the `stretch.img`, `ssh-stretch.id_rsa`, `bzImage` are ready, my recommendation is to create a working directory, create a `bin` directory and copy the patched Syzkaller binary and healer binary to that directory, taking care not to change the `syz-bin` directory structure. The final working directory needs to have the following files.

```
> cd path/to/workdir && ls 
bin  bzImage  stretch.id_rsa  stretch.img
> ls ./bin
healer linux_amd64  syz-repro  syz-symbolize  syz-sysgen
```

Finally, executing following command to start the fuzzing, where `-i` specifies the path to disk image, `-l` specify the path to kernel image, `-t` indicates the fuzzing target and `--ssh-key` specifies the path to ssh key.

```
> # `sudo` maybe needed for `kvm` accessing. 
> healer -i stretch.img --ssh-key stretch.id_rsa -k bzImage -t linux/amd64
```

One can also specify the parallel fuzzing instance (thread) via `-j`, the path to kernel object file (`vmlinux`) and srouce code via `-O` and `-S` so that Healer can symbolize the kernel crash log. See more options via `healer --help`.
If everything works ok, you'll see following log:
``` 

 ___   ___   ______   ________   __       ______   ______
/__/\ /__/\ /_____/\ /_______/\ /_/\     /_____/\ /_____/\
\::\ \\  \ \\::::_\/_\::: _  \ \\:\ \    \::::_\/_\:::_ \ \
 \::\/_\ .\ \\:\/___/\\::(_)  \ \\:\ \    \:\/___/\\:(_) ) )_
  \:: ___::\ \\::___\/_\:: __  \ \\:\ \____\::___\/_\: __ `\ \
   \: \ \\::\ \\:\____/\\:.\ \  \ \\:\/___/\\:\____/\\ \ `\ \ \
    \__\/ \::\/ \_____\/ \__\/\__\/ \_____\/ \_____\/ \_\/ \_\/

2021-02-28 12:24:25 [INFO] Loading target linux/amd64...
2021-02-28 12:24:25 [INFO] Revision: 0085e0
2021-02-28 12:24:25 [INFO] Syscalls: 3581/3960
2021-02-28 12:24:25 [INFO] Initial relations: 0
2021-02-28 12:24:25 [INFO] Booting 14 linux/amd64 on qemu...
2021-02-28 12:24:49 [INFO] code coverage               : enabled
2021-02-28 12:24:49 [INFO] setuid sandbox              : enabled
2021-02-28 12:24:49 [INFO] namespace sandbox           : enabled
2021-02-28 12:24:49 [INFO] fault injection             : enabled
2021-02-28 12:24:49 [INFO] net packet injection        : enabled
2021-02-28 12:24:49 [INFO] net device setup            : enabled
2021-02-28 12:24:49 [INFO] concurrency sanitizer       : enabled
2021-02-28 12:24:49 [INFO] USB emulation               : enabled
2021-02-28 12:24:49 [INFO] hci packet injection        : enabled
2021-02-28 12:24:49 [INFO] wifi device emulation       : enabled
2021-02-28 12:24:50 [INFO] Boot finished, cost 25s
2021-02-28 12:24:50 [INFO] Let the fuzz begin
2021-02-28 12:24:51 [INFO] Fuzzer-0: detect new relation: (socket$inet_tcp, getsockopt$inet6_mreq)
...
```

Again，When the [refactoring](https://github.com/SunHao-0/healer/tree/refactor) code is stable, we will add more information about how to use Healer in the documentation.

## Contributing
All contributions are welcome, if you have a feature request don't hesitate to open an issue!
