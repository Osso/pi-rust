#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <pwd.h>
#include <stdlib.h>
#include <unistd.h>

/* Arm after module/dependency setup. Permit the caller's preceding lookups,
 * then fail every subsequent lookup at the constructor boundary. */
int getpwuid_r(uid_t uid, struct passwd *pwd, char *buffer, size_t size, struct passwd **result) {
    static unsigned long calls;
    const char *marker = getenv("PI_TEST_NSS_FAILURE_FILE");
    if (marker != NULL && access(marker, F_OK) == 0) {
        const char *allowed = getenv("PI_TEST_NSS_ALLOW");
        unsigned long limit = allowed == NULL ? 0 : strtoul(allowed, NULL, 10);
        calls++;
        if (calls > limit) {
            *result = NULL;
            errno = EIO;
            return EIO;
        }
    }
    typedef int (*lookup)(uid_t, struct passwd *, char *, size_t, struct passwd **);
    lookup original = (lookup)dlsym(RTLD_NEXT, "getpwuid_r");
    return original(uid, pwd, buffer, size, result);
}
