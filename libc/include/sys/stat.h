#ifndef _SYS_STAT_H
#define _SYS_STAT_H

#include <stdint.h>

struct stat {
    uint32_t st_size;
    uint32_t st_mode;
};

#define S_IFMT  0170000
#define S_IFDIR 0040000
#define S_IFREG 0100000

#define S_ISDIR(m) (((m) & S_IFMT) == S_IFDIR)
#define S_ISREG(m) (((m) & S_IFMT) == S_IFREG)

int stat(const char *path, struct stat *buf);
int fstat(int fd, struct stat *buf);
int mkdir(const char *path, uint32_t mode);

#endif
