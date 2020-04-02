# Healer
[![Build](https://github.com/SunHao-0/healer/workflows/Build/badge.svg)](https://github.com/SunHao-0/healer/actions?query=workflow%3ABuild) 
 [![Check](https://github.com/SunHao-0/healer/workflows/Check/badge.svg)](https://github.com/SunHao-0/healer/actions?query=workflow%3ACheck)

Healer is a kernel fuzzer inspired by [Syzkaller](https://github.com/google/syzkaller).
As a system call fuzzer, its basic workflow is generating system calls sequence, executing
calls, collecting and analyzing feedback as well as monitoring crash. The difference between 
healer and traditional fuzzer is that healer is aware of type, semantic constraint
of parametor and relation between different system calls, which enable healer generating high
quality test case and fuzzing more efficient.

Core components in healer are:
1. FOTS, a fuzzing oriented interface discription language. [see more](./fots/Readme.md)
2. Core algorithm, including relation analyzing, call sequence generating, translating... [see more](./core/Readme.md)
3. Executor, support `interprete` and `jit` executing, `direct` feature is on working
4. Related tools, such as reportor, translator, exec... 
5. Fuzzer, built on top core and fots.

## Install 
Currently, healer only intends to run on and fuzz linux. Therefore following guide only works for linux.

### Install Rust 
Healer is written in pure rust, except for some ffi libraries. Go to official site of [rust](https://www.rust-lang.org/),
download and install rustc and cargo.
On linux, following command makes life easier:
``` bash
> sudo apt update && sudo apt upgrade 
> curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
> rustc --version # check install
```

### Build Healer
1. Clone or download healer.
2. Build healer with following commands:
``` bash
> cd healer
> cargo build --release
```
3. After build finished, following executable files should be available in `target/release` directory.
    - *fuzzer* and *executor*, most important tools.
    - *fots*, compiler of FOTS.
    - *kill*, current useful tool for stop fuzzer, don't use CTRL-C to terminate healer.
    - other tools, such as *gen*, *trans*...


## Fuzz
To fuzz linux with healer, a configure `healer-fuzzer.toml` file is necessary. Following 
guide introduces best practice in using healer.

### Prepare a Working Directory
First, create a working directory, but do not create it inside healer source code dir. Then copy 
executable files we've built to a sub-directory called `bin` and copy fots files to `descs` in working dir.
``` bash
> # out of healer directory
> mkdir work-dir &&  cd work-dir
> mkdir bin && cp path/to/healer/target/release/executable ./bin
> mkdir descs && cp path/to/fots_file/*.fots ./descs
```
### Prepare Kernel
Create a sub-directort `target` inside work-dir and build kernel bzImage and stretch.img there following this [guide](https://github.com/google/syzkaller/blob/master/docs/linux/setup_ubuntu-host_qemu-vm_x86-64-kernel.md). 


### Prepare Config file 
Build fots files in `desc` directory using following commands:
``` bash
> # -d specifies the directort to search fots file, -o soecifies output file.
> ./bin/fots build -d desc -o sys
````

Modify config options in your `healer-fuzzer.toml` based on following template.
``` toml
fots_bin = "./sys"
vm_num = 2

[guest]
os = "linux"
arch = "amd64"
platform = "qemu"

[qemu]
cpu_num = 1
mem_size = 1024
image = "./target/stretch.img"
kernel = "./target/bzImage-bug"
wait_boot_time = 5

[ssh]
key_path = "./target/stretch.id_rsa"

[executor]
path = "./bin/executor"
host_ip="localhost" 

[sampler]
sample_interval=15
report_interval=5

[mail]
sender="healer-stats@outlook.com"
receivers=["xx@outlook.com"]
```
Meaning of each option:
- *fots_bin*: path to compiled fots file.
- *vm_num*: number of virtual machine to be used.
- *guest* fragment defines (os,arch,platform). (linux, amd64, qemu) is supported now.
- *qemu* fragment defines arguments passed to qemu, *wait_boot_time* is duration in seconds for waiting kernel to boot up  
- *ssh* fragment defines arguments passed ssh(internal used), key_path is path to secret key file generated during kernel building step.
- *executor* define arguments passed to executor and path of executor, path is the only needed option for now.
- *sampler* data samplers config options
- *mail* healer support auto report via outlook email, config your sender and receivers list, transfer password of 
sender via env var while starting healer. 

### Fuzzing
After preparing everything we need, just run following command:
``` bash 
> sudo HEALER_MAIL_PASSWD="..."  ./bin/fuzzer 
```
If everything works ok, you'll see following msg:
``` bash
 ___   ___   ______   ________   __       ______   ______
/__/\ /__/\ /_____/\ /_______/\ /_/\     /_____/\ /_____/\
\::\ \\  \ \\::::_\/_\::: _  \ \\:\ \    \::::_\/_\:::_ \ \
 \::\/_\ .\ \\:\/___/\\::(_)  \ \\:\ \    \:\/___/\\:(_) ) )_
  \:: ___::\ \\::___\/_\:: __  \ \\:\ \____\::___\/_\: __ `\ \
   \: \ \\::\ \\:\____/\\:.\ \  \ \\:\/___/\\:\____/\\ \ `\ \ \
    \__\/ \::\/ \_____\/ \__\/\__\/ \_____\/ \_____\/ \_\/ \_\/


 2020-03-26T11:10:38.134 INFO  fuzzer > Pid: 20807
 2020-03-26T11:10:38.134 INFO  fuzzer > Work-dir: .
 2020-03-26T11:10:38.148 INFO  fuzzer > Corpus: 0
 2020-03-26T11:10:38.148 INFO  fuzzer > Booting 2 linux/amd64 on qemu ...
 2020-03-26T11:10:55.421 INFO  fuzzer::exec > connected from: 127.0.0.1:44098
 2020-03-26T11:10:55.428 INFO  fuzzer::exec > connected from: 127.0.0.1:43476
 2020-03-26T11:10:55.428 INFO  fuzzer       > Boot finished, cost 17s.
 2020-03-26T11:11:10.429 INFO  fuzzer::stats > corpus 214,blocks 9865,branches 11604,candidates 0,normal_case 214,failed_case 0,crashed_case 0
```
Do not use CTRL-C to kill healer, use `bin/kill` instead.
``` bash
> # pass pid of healer to kill tool
> ./bin/kill 20807
```

After fuzzing finished, *report* tool can be used to generate readable fuzz result report with following command:
``` bash 
> # [creashes] is directory storing every crash, normal_case.json and faile_case.json stores test cases, report is written to report directory
> ./bin/report -c [crashes] -n normal_case.json -f .failed_case.json -o report 
> # If mdbook is not found, use cargo to install
> cargo install mdbook
> # then  
> mdbook build -o ./report 
```
