#ifndef __SORTING_H__
#define __SORTING_H__ "version.h"

#include <linux/version.h>

#ifdef __cplusplus
extern "C" {
#endif

int linux_version_code();

int linux_version_major();

int linux_version_patchlevel();

int linux_version_sublevel();

#ifdef __cplusplus
}
#endif
#endif