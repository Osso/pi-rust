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

/* Only selected real artifact descriptors are affected, never protocol pipes. */
static int synced_descriptor = -1;
static int matches_path(const char *path) {
    const char *needle = getenv("PI_BASH_SPILL_PATH");
    if (!needle) needle = "/pi-bash-";
    return getenv("PI_BASH_SPILL_EXACT") ? !strcmp(path, needle) : strstr(path, needle) != NULL;
}
static int matches_descriptor(int fd) {
    struct stat status;
    if (fstat(fd, &status) || (!S_ISREG(status.st_mode) && !S_ISDIR(status.st_mode))) return 0;
    char link[64], path[4096];
    snprintf(link, sizeof(link), "/proc/self/fd/%d", fd);
    ssize_t length = readlink(link, path, sizeof(path) - 1);
    if (length < 0) return 0;
    path[length] = 0;
    return matches_path(path);
}
static int requests_fault(const char *kind, int fd) {
    const char *wanted = getenv("PI_BASH_SPILL_FAIL");
    if (!wanted) return 0;
    if (!strcmp(wanted, kind)) return 1;
    if (!strcmp(wanted, "write-close")) return !strcmp(kind, "write") || !strcmp(kind, "close");
    if (!strcmp(wanted, "fsync-close")) return !strcmp(kind, "fsync") || (!strcmp(kind, "close") && fd == synced_descriptor);
    return !strcmp(wanted, "close-after-sync") && !strcmp(kind, "close") && fd == synced_descriptor;
}
static int descriptor_fault(int fd, const char *kind) {
    return requests_fault(kind, fd) && matches_descriptor(fd);
}
ssize_t write(int fd, const void *data, size_t size) {
    if (descriptor_fault(fd, "write")) { errno = EIO; return -1; }
    ssize_t (*real)(int, const void *, size_t) = dlsym(RTLD_NEXT, "write");
    return real(fd, data, size);
}
ssize_t writev(int fd, const struct iovec *data, int count) {
    if (descriptor_fault(fd, "write")) { errno = EIO; return -1; }
    ssize_t (*real)(int, const struct iovec *, int) = dlsym(RTLD_NEXT, "writev");
    return real(fd, data, count);
}
int fsync(int fd) {
    int match = matches_descriptor(fd);
    if (match) synced_descriptor = fd;
    if (descriptor_fault(fd, "fsync")) { errno = EIO; return -1; }
    int (*real)(int) = dlsym(RTLD_NEXT, "fsync");
    return real(fd);
}
int rename(const char *from, const char *to) {
    if (requests_fault("rename", -1) && matches_path(from)) { errno = EIO; return -1; }
    int (*real)(const char *, const char *) = dlsym(RTLD_NEXT, "rename");
    return real(from, to);
}
int close(int fd) {
    int fail = descriptor_fault(fd, "close");
    int (*real)(int) = dlsym(RTLD_NEXT, "close");
    int result = real(fd);
    if (fd == synced_descriptor) synced_descriptor = -1;
    if (fail && !result) { errno = EBADF; return -1; }
    return result;
}

/* libuv bypasses libc close. Invalidate only the selected descriptor and let
 * the actual libuv request produce EBADF (sync callback=NULL or async). */
int uv_fs_close(uv_loop_t *loop, uv_fs_t *request, uv_file file, uv_fs_cb callback) {
    int (*real)(uv_loop_t *, uv_fs_t *, uv_file, uv_fs_cb) = dlsym(RTLD_NEXT, "uv_fs_close");
    if (descriptor_fault(file, "close")) {
        int (*close_real)(int) = dlsym(RTLD_NEXT, "close");
        close_real(file);
        if (file == synced_descriptor) synced_descriptor = -1;
        return real(loop, request, -1, callback);
    }
    if (file == synced_descriptor) synced_descriptor = -1;
    return real(loop, request, file, callback);
}
