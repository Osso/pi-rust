#include "grep_read_failure.c"
#include <fcntl.h>
#include <stdarg.h>

/* Fail only the fixture artifact's read open, after bash opened it for writing.
 * The same syscall failure distinguishes Promise setup from a later abort event. */
static int fail_artifact_read(const char *path, int flags) {
    const char *target = getenv("PI_BASH_FAIL_OPEN_PATH");
    if (target && strcmp(path, target) == 0 && (flags & O_ACCMODE) == O_RDONLY) {
        errno = ENOENT;
        return 1;
    }
    return 0;
}
int open(const char *path, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = va_arg(args, mode_t);
        va_end(args);
    }
    if (fail_artifact_read(path, flags)) return -1;
    int (*open_real)(const char *, int, ...) = dlsym(RTLD_NEXT, "open");
    return open_real(path, flags, mode);
}
int open64(const char *path, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = va_arg(args, mode_t);
        va_end(args);
    }
    if (fail_artifact_read(path, flags)) return -1;
    int (*open_real)(const char *, int, ...) = dlsym(RTLD_NEXT, "open64");
    return open_real(path, flags, mode);
}
