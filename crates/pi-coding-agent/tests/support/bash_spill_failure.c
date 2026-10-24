#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/uio.h>
#include <unistd.h>
#include <uv.h>

/* Fault only actual pi-bash spill-file syscalls; protocol pipes/module loads stay intact. */
static int spill_fault(int fd, const char *kind) {
    const char *wanted = getenv("PI_BASH_SPILL_FAIL");
    if (!wanted || strcmp(wanted, kind)) return 0;
    struct stat status;
    if (fstat(fd, &status) || !S_ISREG(status.st_mode)) return 0;
    char link[64], path[4096];
    snprintf(link, sizeof(link), "/proc/self/fd/%d", fd);
    ssize_t length = readlink(link, path, sizeof(path) - 1);
    if (length < 0) return 0;
    path[length] = 0;
    const char *needle = getenv("PI_BASH_SPILL_PATH");
    return strstr(path, needle ? needle : "/pi-bash-") != NULL;
}
ssize_t write(int fd, const void *data, size_t size) {
    if (spill_fault(fd, "write")) { errno = EIO; return -1; }
    ssize_t (*real)(int, const void *, size_t) = dlsym(RTLD_NEXT, "write");
    return real(fd, data, size);
}
ssize_t writev(int fd, const struct iovec *data, int count) {
    if (spill_fault(fd, "write")) { errno = EIO; return -1; }
    ssize_t (*real)(int, const struct iovec *, int) = dlsym(RTLD_NEXT, "writev");
    return real(fd, data, count);
}
int close(int fd) {
    int fail = spill_fault(fd, "close");
    int (*real)(int) = dlsym(RTLD_NEXT, "close");
    int result = real(fd);
    if (fail && !result) { errno = EBADF; return -1; }
    return result;
}

/* libuv uses a direct close syscall, not libc's close symbol. Invalidate the
 * actual spill descriptor, then let its asynchronous close report real EBADF. */
int uv_fs_close(uv_loop_t *loop, uv_fs_t *request, uv_file file, uv_fs_cb callback) {
    int (*real)(uv_loop_t *, uv_fs_t *, uv_file, uv_fs_cb) = dlsym(RTLD_NEXT, "uv_fs_close");
    if (spill_fault(file, "close")) {
        int (*close_real)(int) = dlsym(RTLD_NEXT, "close");
        close_real(file);
        return real(loop, request, -1, callback);
    }
    return real(loop, request, file, callback);
}
