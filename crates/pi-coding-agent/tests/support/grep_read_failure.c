#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

/* Inject a real pipe/socket read failure only after the fixture's marker arrives.
 * File/module reads and the oracle's stdin are untouched. No production seam. */
ssize_t read(int fd, void *buffer, size_t count) {
    ssize_t (*read_real)(int, void *, size_t) = dlsym(RTLD_NEXT, "read");
    ssize_t result = read_real(fd, buffer, count);
    const char *marker = getenv("PI_GREP_READ_FAILURE_MARKER");
    struct stat status;
    if (fd > STDERR_FILENO && result > 0 && marker &&
        fstat(fd, &status) == 0 && (S_ISFIFO(status.st_mode) || S_ISSOCK(status.st_mode)) &&
        memmem(buffer, (size_t)result, marker, strlen(marker))) {
        errno = EIO;
        return -1;
    }
    return result;
}
