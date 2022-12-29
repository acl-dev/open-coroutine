#ifndef UNTITLED_LIBRARY_H
#define UNTITLED_LIBRARY_H

#include <limits>
#include <dlfcn.h>
#include <unistd.h>
#include <sys/time.h>
#include <sys/types.h>
#include <sys/socket.h>
#include <sys/ioctl.h>
#include "../base-coroutine/include/libcoroutine.h"

uint64_t ns_now() {
    struct timeval tp;
    tp.tv_sec = 0;
    tp.tv_usec = 0;
    gettimeofday(&tp, nullptr);
    return tp.tv_sec * 1000000000 + tp.tv_usec * 1000;
}

unsigned int sleep(unsigned int secs) {
    struct timespec rqtp;
    rqtp.tv_sec = secs;
    rqtp.tv_nsec = 0;

    struct timespec rmtp;
    rmtp.tv_sec = 0;
    rmtp.tv_nsec = 0;

    nanosleep(&rqtp, &rmtp);
    return rmtp.tv_sec;
}

int usleep(useconds_t microseconds) {
    struct timespec rqtp;
    rqtp.tv_sec = microseconds / 1000000;
    rqtp.tv_nsec = (microseconds % 1000000) * 1000;

    struct timespec rmtp;
    rmtp.tv_sec = 0;
    rmtp.tv_nsec = 0;

    nanosleep(&rqtp, &rmtp);
    return rmtp.tv_sec * 1000000 + rmtp.tv_nsec / 1000;
}

#endif //UNTITLED_LIBRARY_H
