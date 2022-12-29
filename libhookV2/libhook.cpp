#include "libhook.h"

const auto sys_nanosleep = (int (*)(const struct timespec *, struct timespec *)) dlsym(RTLD_NEXT, "nanosleep");

int nanosleep(const struct timespec *rqtp, struct timespec *rmtp) {
    uint64_t timeout_time = ns_now() + rqtp->tv_sec * 1000000000 + rqtp->tv_nsec;
    for (;;) {
        try_timeout_schedule(timeout_time);
        // 可能schedule完还剩一些时间，此时本地队列没有任务可做
        uint64_t left_time = timeout_time - ns_now();
        if (left_time <= 0) {
            return 0;
        }

        struct timespec new_rqtp;
        new_rqtp.tv_sec = left_time / 1000000000;
        new_rqtp.tv_nsec = left_time % 1000000000;
        if ((sys_nanosleep)(&new_rqtp, rmtp) == 0) {
            return 0;
        }
    }
}

const auto sys_connect = (int (*)(int, const struct sockaddr *, socklen_t)) dlsym(RTLD_NEXT, "connect");
const int BLOCKING = 0;
const int NONBLOCKING = 1;

int connect(int socket, const struct sockaddr *address, socklen_t address_len) {
    try_timed_schedule(std::numeric_limits<uint64_t>::max());
    //todo 非阻塞实现
    return (sys_connect)(socket, address, address_len);
}

const auto sys_listen = (int (*)(int, int)) dlsym(RTLD_NEXT, "listen");

int listen(int socket, int backlog) {
    try_timed_schedule(std::numeric_limits<uint64_t>::max());
    return (sys_listen)(socket, backlog);
}

const auto sys_accept = (int (*)(int, struct sockaddr *, socklen_t *)) dlsym(RTLD_NEXT, "accept");

int accept(int socket, struct sockaddr *address, socklen_t *address_len) {
    try_timed_schedule(std::numeric_limits<uint64_t>::max());
    //todo 非阻塞实现
    return (sys_accept)(socket, address, address_len);
}

const auto sys_send = (ssize_t (*)(int, const void *, size_t, int)) dlsym(RTLD_NEXT, "send");

ssize_t send(int socket, const void *buffer, size_t length, int flags) {
    try_timed_schedule(std::numeric_limits<uint64_t>::max());
    //todo 非阻塞实现
    return (sys_send)(socket, buffer, length, flags);
}

const auto sys_recv = (ssize_t (*)(int, void *, size_t, int)) dlsym(RTLD_NEXT, "recv");

ssize_t recv(int socket, void *buffer, size_t length, int flags) {
    try_timed_schedule(std::numeric_limits<uint64_t>::max());
    //todo 非阻塞实现
    return (sys_recv)(socket, buffer, length, flags);
}