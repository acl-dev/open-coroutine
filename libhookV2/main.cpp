#include "../base-coroutine/include/libcoroutine.h"
#include <iostream>
#include <unistd.h>
#include <sys/time.h>

uint64_t ns_now() {
    struct timeval tp;
    tp.tv_sec = 0;
    tp.tv_usec = 0;
    gettimeofday(&tp, nullptr);
    return tp.tv_sec * 1000000000 + tp.tv_usec * 1000;
}

void *co_main(const Yielder<void *, void, void *> *yielder, void *param) {
    std::cout << "Hello, Coroutine!" << std::endl;
    return nullptr;
}

int main() {
    coroutine_crate(co_main, nullptr, 2048);
//    try_timeout_schedule(std::numeric_limits<uint64_t>::max());
    auto start = ns_now();
    struct timespec rqtp;
    rqtp.tv_sec = 1;
    rqtp.tv_nsec = 0;
    int result = nanosleep(&rqtp, nullptr);
    std::cout << result << " cost " << ns_now() - start << "ns" << std::endl;
    return 0;
}
