/* Owned, input-free OS diagnostic. Not a native product/worker acceptance test. */
#define _DEFAULT_SOURCE 1
#define _DARWIN_C_SOURCE 1
#include <errno.h>
#include <inttypes.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/resource.h>
#include <unistd.h>
#ifdef __APPLE__
#include <mach/mach.h>
#endif

static uint64_t virtual_bytes(int *available) {
#ifdef __APPLE__
    struct mach_task_basic_info info;
    mach_msg_type_number_t count = MACH_TASK_BASIC_INFO_COUNT;
    *available = task_info(mach_task_self(), MACH_TASK_BASIC_INFO,
                           (task_info_t)&info, &count) == KERN_SUCCESS;
    return *available ? (uint64_t)info.virtual_size : 0;
#else
    FILE *input = fopen("/proc/self/statm", "r");
    uint64_t pages = 0;
    long page_size = sysconf(_SC_PAGESIZE);
    *available = input && page_size > 0 && fscanf(input, "%" SCNu64, &pages) == 1
        && pages <= UINT64_MAX / (uint64_t)page_size;
    if (input) fclose(input);
    return *available ? pages * (uint64_t)page_size : 0;
#endif
}

int main(int argc, char **argv) {
    const uint64_t requested = UINT64_C(256) * 1024 * 1024;
    const size_t excessive = (size_t)(requested * 2);
    if (sizeof(size_t) < 8 || argc != 2
        || (strcmp(argv[1], "self") && strcmp(argv[1], "inherited"))) return 64;
    signal(SIGALRM, SIG_DFL);
    alarm(3);
    int vm_available = 0;
    uint64_t vm = virtual_bytes(&vm_available);
    int attempted = strcmp(argv[1], "self") == 0;
    int installed = 0, install_errno = 0;
    struct rlimit limit = {(rlim_t)requested, (rlim_t)requested};
    if (attempted) {
        errno = 0;
        installed = setrlimit(RLIMIT_AS, &limit) == 0;
        install_errno = installed ? 0 : errno;
    }
    struct rlimit actual = {0, 0};
    int observed = getrlimit(RLIMIT_AS, &actual) == 0;
    int exact = observed && actual.rlim_cur == requested && actual.rlim_max == requested;
    int malloc_denied = 0, mmap_denied = 0, malloc_errno = 0, mmap_errno = 0;
    /* Never attempt large mappings unless the exact hard/soft limit is observed.
       Successful mappings are immediately released without touching their pages. */
    if (exact) {
        errno = 0;
        void *heap = malloc(excessive);
        malloc_denied = heap == NULL;
        malloc_errno = malloc_denied ? errno : 0;
        free(heap);
        errno = 0;
        void *mapping = mmap(NULL, excessive, PROT_READ | PROT_WRITE,
                             MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
        mmap_denied = mapping == MAP_FAILED;
        mmap_errno = mmap_denied ? errno : 0;
        if (!mmap_denied && munmap(mapping, excessive) != 0) return 65;
    }
    char output[2048];
    int length = snprintf(output, sizeof(output),
        "{\"schema\":1,\"mode\":\"%s\",\"requestedBytes\":%" PRIu64
        ",\"vmObservationAvailable\":%s,\"virtualBytesBefore\":%" PRIu64
        ",\"setLimitAttempted\":%s,\"setLimitSucceeded\":%s,\"setLimitErrno\":%d"
        ",\"limitObservationAvailable\":%s,\"observedSoftBytes\":%" PRIu64
        ",\"observedHardBytes\":%" PRIu64 ",\"exactLimitInstalled\":%s"
        ",\"allocationProbeAttempted\":%s,\"mallocDenied\":%s,\"mallocErrno\":%d"
        ",\"mmapDenied\":%s,\"mmapErrno\":%d,\"nativeAcceptance\":false}\n",
        argv[1], requested, vm_available ? "true" : "false", vm,
        attempted ? "true" : "false", attempted ? (installed ? "true" : "false") : "null",
        install_errno, observed ? "true" : "false", (uint64_t)actual.rlim_cur,
        (uint64_t)actual.rlim_max, exact ? "true" : "false", exact ? "true" : "false",
        exact ? (malloc_denied ? "true" : "false") : "null", malloc_errno,
        exact ? (mmap_denied ? "true" : "false") : "null", mmap_errno);
    if (length <= 0 || (size_t)length >= sizeof(output)) return 66;
    size_t sent = 0;
    while (sent < (size_t)length) {
        ssize_t count = write(STDOUT_FILENO, output + sent, (size_t)length - sent);
        if (count < 0 && errno == EINTR) continue;
        if (count <= 0) return 67;
        sent += (size_t)count;
    }
    return 0;
}
