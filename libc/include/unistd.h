#ifndef _UNISTD_H
#define _UNISTD_H

#include <stddef.h>

/* Standard POSIX syscall wrappers */
int     close(int fd);
long    read(int fd, void *buf, size_t count);
long    write(int fd, const void *buf, size_t count);
long    lseek(int fd, long offset, int whence);
int     getpid(void);

/* MYNEWOS specific */
int     getkey(void);
long    getticks(void);
int     screen_blit(const void *buf, int w, int h);
int     spawn(const char *path);

#endif /* _UNISTD_H */
