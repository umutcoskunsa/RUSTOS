#ifndef _STDLIB_H
#define _STDLIB_H

#include <stddef.h>

/* Memory management */
void  *malloc (size_t size);
void  *calloc (size_t nmemb, size_t size);
void  *realloc(void *ptr, size_t size);
void   free   (void *ptr);

/* Program control */
void   exit   (int status) __attribute__((noreturn));
int    atexit (void (*func)(void));
void   abort  (void) __attribute__((noreturn));

/* Conversions */
int    atoi  (const char *s);
long   atol  (const char *s);
double atof  (const char *s);
long   strtol(const char *s, char **end, int base);
unsigned long strtoul(const char *s, char **end, int base);

/* Utilities */
int    abs   (int x);
long   labs  (long x);
int    rand  (void);
void   srand (unsigned int seed);

/* Environment */
char  *getenv(const char *name);

#define EXIT_SUCCESS 0
#define EXIT_FAILURE 1
#define RAND_MAX     32767

#endif /* _STDLIB_H */
