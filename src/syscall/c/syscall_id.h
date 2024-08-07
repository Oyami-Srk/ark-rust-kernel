/* Already Implemented */

/* File system */
#define SYS_openat 56
#define SYS_read 63
#define SYS_write 64
#define SYS_lseek 62
#define SYS_close 57
#define SYS_mkdirat 34
#define SYS_mount 40
#define SYS_fstat 80
#define SYS_readv 65
#define SYS_writev 66
#define SYS_newfstatat 79
#define SYS_getdents64 61
#define SYS_linkat 37
#define SYS_pipe2 59

/* Process */
#define SYS_exit 93
#define SYS_clone 220
#define SYS_execve 221
#define SYS_wait4 260
#define SYS_getpid 172
#define SYS_getppid 173
#define SYS_sched_yield 124

/* Memory */
#define SYS_brk 214
#define SYS_mmap 222
#define SYS_munmap 215

/* ARK Custom Syscall */
#define SYS_ark_sleep_ticks 1002
#define SYS_ark_breakpoint 20010125

/* Misc */
#define SYS_uname 160
#define SYS_getcwd 17
#define SYS_chdir 49

/* Dummy stub */
#define SYS_getuid 174
#define SYS_geteuid 175
#define SYS_getgid 176
#define SYS_getegid 177
#define SYS_gettid 178
#define SYS_setuid 146
#define SYS_setgid 144
#define SYS_exit_group 94
#define SYS_set_tid_address 96
#define SYS_ioctl 29
#define SYS_fcntl64 25
#define SYS_clock_gettime 113

/* Going to be Implemented */
#define SYS_dup 23
#define SYS_rt_sigaction 134
#define SYS_rt_sigprocmask 135

/* Not too urgent to be Implemented */
#define SYS_dup3 24
#define SYS_unlinkat 35
#define SYS_umount2 39
#define SYS_times 153
#define SYS_gettimeofday 169
#define SYS_nanosleep 101
#define SYS_ppoll 73
