#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <pwd.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/* Force only the exercised lookup; Node/tsx imports initialize with HOME set. */
int getpwuid_r(uid_t uid, struct passwd *pwd, char *buffer, size_t size, struct passwd **result) {
    const char *fail = getenv("PI_TEST_FAIL_PASSWD");
    const char *marker = getenv("PI_TEST_PASSWD_FAILURE_FILE");
    if ((fail != NULL && strcmp(fail, "1") == 0) || (marker != NULL && access(marker, F_OK) == 0)) {
        *result = NULL;
        errno = EIO;
        return EIO;
    }
    typedef int (*lookup)(uid_t, struct passwd *, char *, size_t, struct passwd **);
    lookup original = (lookup)dlsym(RTLD_NEXT, "getpwuid_r");
    int status = original(uid, pwd, buffer, size, result);
    const char *home = getenv("PI_TEST_PASSWD_HOME");
    if (status == 0 && *result != NULL && home != NULL) {
        pwd->pw_dir = (char *)home;
    }
    return status;
}
