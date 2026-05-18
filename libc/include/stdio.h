#ifndef _STDIO_H
#define _STDIO_H

#include <stddef.h>
#include <stdarg.h>

/* ------------------------------------------------------------------ */
/* FILE type — backed by a file descriptor                              */
/* ------------------------------------------------------------------ */
typedef struct {
    int   fd;          /* kernel file descriptor (-1 = closed) */
    int   eof;         /* non-zero when end-of-file reached    */
    int   error;       /* non-zero on error                    */
    int   mode;        /* 0=read, 1=write                      */
} FILE;

extern FILE *stdin;
extern FILE *stdout;
extern FILE *stderr;

#define stdin  stdin
#define stdout stdout
#define stderr stderr

/* ------------------------------------------------------------------ */
/* Seek whence constants                                               */
/* ------------------------------------------------------------------ */
#define SEEK_SET 0
#define SEEK_CUR 1
#define SEEK_END 2

#define EOF   (-1)
#define BUFSIZ 1024

/* ------------------------------------------------------------------ */
/* printf family                                                        */
/* ------------------------------------------------------------------ */
int  printf (const char *fmt, ...);
int  fprintf(FILE *stream, const char *fmt, ...);
int  sprintf(char *buf, const char *fmt, ...);
int  snprintf(char *buf, size_t n, const char *fmt, ...);
int  sscanf(const char *str, const char *fmt, ...);
int  vprintf (const char *fmt, va_list ap);
int  vfprintf(FILE *stream, const char *fmt, va_list ap);
int  vsprintf(char *buf, const char *fmt, va_list ap);
int  vsnprintf(char *buf, size_t n, const char *fmt, va_list ap);

/* ------------------------------------------------------------------ */
/* Character I/O                                                        */
/* ------------------------------------------------------------------ */
int  putchar(int c);
int  puts   (const char *s);
int  fputc  (int c, FILE *stream);
int  fputs  (const char *s, FILE *stream);
int  getchar(void);
int  fgetc  (FILE *stream);

/* ------------------------------------------------------------------ */
/* File I/O                                                             */
/* ------------------------------------------------------------------ */
FILE *fopen (const char *path, const char *mode);
int   fclose(FILE *stream);
size_t fread(void *ptr, size_t size, size_t nmemb, FILE *stream);
size_t fwrite(const void *ptr, size_t size, size_t nmemb, FILE *stream);
int   fseek (FILE *stream, long offset, int whence);
long  ftell (FILE *stream);
int   feof  (FILE *stream);
int   ferror(FILE *stream);
void  rewind(FILE *stream);
void  perror(const char *s);
int   fflush(FILE *stream);

#endif /* _STDIO_H */
