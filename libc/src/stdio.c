/*
 * stdio.c — Standard I/O functions for MYNEWOS libc
 */

#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <stdarg.h>

/* Raw syscall wrappers from syscall.S */
extern long write(int fd, const void *buf, size_t count);
extern long read(int fd, void *buf, size_t count);
extern int open(const char *name, int flags);
extern int close(int fd);
extern long lseek(int fd, long offset, int whence);
extern int unlink(const char *name);
extern int rename(const char *oldname, const char *newname);

static FILE _stdin  = { 0, 0, 0, 0 };
static FILE _stdout = { 1, 0, 0, 1 };
static FILE _stderr = { 2, 0, 0, 1 };

FILE *stdin  = &_stdin;
FILE *stdout = &_stdout;
FILE *stderr = &_stderr;

/* ------------------------------------------------------------------ */
/* Primitive Character I/O                                            */
/* ------------------------------------------------------------------ */

int putchar(int c) {
    unsigned char ch = (unsigned char)c;
    write(1, &ch, 1);
    return c;
}

int puts(const char *s) {
    size_t len = strlen(s);
    write(1, s, len);
    write(1, "\n", 1);
    return 0;
}

int fputc(int c, FILE *stream) {
    unsigned char ch = (unsigned char)c;
    if (write(stream->fd, &ch, 1) < 0) {
        stream->error = 1;
        return EOF;
    }
    return c;
}

int fputs(const char *s, FILE *stream) {
    size_t len = strlen(s);
    if (write(stream->fd, s, len) < 0) {
        stream->error = 1;
        return EOF;
    }
    return 0;
}

int getchar(void) {
    unsigned char ch;
    if (read(0, &ch, 1) <= 0) return EOF;
    return (int)ch;
}

int fgetc(FILE *stream) {
    unsigned char ch;
    if (read(stream->fd, &ch, 1) <= 0) {
        stream->eof = 1;
        return EOF;
    }
    return (int)ch;
}

/* ------------------------------------------------------------------ */
/* File Management                                                    */
/* ------------------------------------------------------------------ */

FILE *fopen(const char *path, const char *mode) {
    int fd = open(path, 0); // Our current 'open' ignores flags and just reads
    if (fd < 0) {
        printf("libc: fopen failed for '%s'\n", path);
        return NULL;
    }

    FILE *f = (FILE *)malloc(sizeof(FILE));
    if (!f) return NULL;

    f->fd = fd;
    f->eof = 0;
    f->error = 0;
    f->mode = (mode[0] == 'w' || mode[0] == 'a') ? 1 : 0;
    return f;
}

int fclose(FILE *stream) {
    if (!stream) return EOF;
    close(stream->fd);
    free(stream);
    return 0;
}

size_t fread(void *ptr, size_t size, size_t nmemb, FILE *stream) {
    if (size == 0 || nmemb == 0) return 0;
    long total = size * nmemb;
    long bytes_read = read(stream->fd, ptr, total);
    if (bytes_read <= 0) {
        if (bytes_read == 0) stream->eof = 1;
        else stream->error = 1;
        return 0;
    }
    return (size_t)(bytes_read / size);
}

size_t fwrite(const void *ptr, size_t size, size_t nmemb, FILE *stream) {
    if (size == 0 || nmemb == 0) return 0;
    long total = size * nmemb;
    long bytes_written = write(stream->fd, ptr, total);
    if (bytes_written < 0) {
        stream->error = 1;
        return 0;
    }
    return (size_t)(bytes_written / size);
}

int feof(FILE *stream)   { return stream->eof; }
int ferror(FILE *stream) { return stream->error; }

int fseek(FILE *stream, long offset, int whence) {
    long res = lseek(stream->fd, offset, whence);
    if (res < 0) return -1;
    stream->eof = 0;
    return 0;
}

long ftell(FILE *stream) {
    return lseek(stream->fd, 0, 1); // SEEK_CUR
}

void rewind(FILE *stream) {
    fseek(stream, 0, 0); // SEEK_SET
}

/* ------------------------------------------------------------------ */
/* printf (Minimal Implementation)                                     */
/* ------------------------------------------------------------------ */

static void itoa(long n, char *s, int base) {
    static char digits[] = "0123456789abcdef";
    char buf[64];
    int i = 0;
    int negative = 0;
    unsigned long un = (unsigned long)n;

    if (base == 10 && n < 0) {
        negative = 1;
        un = (unsigned long)-n;
    }

    if (un == 0) buf[i++] = '0';
    while (un > 0) {
        buf[i++] = digits[un % base];
        un /= base;
    }
    if (negative) buf[i++] = '-';
    
    int j = 0;
    while (i > 0) s[j++] = buf[--i];
    s[j] = '\0';
}

int vsnprintf(char *str, size_t size, const char *format, va_list ap) {
    if (size == 0) return 0;
    int count = 0;
    const char *p = format;
    char *d = str;

    #define VSNP_PUT(ch) do { if ((size_t)count < size - 1) { *d++ = (ch); } count++; } while(0)

    while (*p) {
        if (*p != '%') {
            VSNP_PUT(*p);
            p++;
            continue;
        }
        p++; /* skip '%' */

        /* --- parse flags --- */
        int flag_minus = 0, flag_zero = 0, flag_plus = 0, flag_space = 0;
        for (;;) {
            if      (*p == '-') { flag_minus = 1; p++; }
            else if (*p == '0') { flag_zero  = 1; p++; }
            else if (*p == '+') { flag_plus  = 1; p++; }
            else if (*p == ' ') { flag_space = 1; p++; }
            else if (*p == '#') { p++; } /* ignore '#' for now */
            else break;
        }

        /* --- parse width --- */
        int width = 0;
        if (*p == '*') {
            width = va_arg(ap, int);
            if (width < 0) { flag_minus = 1; width = -width; }
            p++;
        } else {
            while (*p >= '0' && *p <= '9') {
                width = width * 10 + (*p - '0');
                p++;
            }
        }

        /* --- parse precision --- */
        int has_prec = 0, prec = 0;
        if (*p == '.') {
            has_prec = 1;
            p++;
            if (*p == '*') {
                prec = va_arg(ap, int);
                if (prec < 0) { has_prec = 0; prec = 0; }
                p++;
            } else {
                while (*p >= '0' && *p <= '9') {
                    prec = prec * 10 + (*p - '0');
                    p++;
                }
            }
        }

        /* --- parse length modifier --- */
        int is_long = 0, is_longlong = 0, is_short = 0, is_char = 0, is_size = 0;
        if (*p == 'l') {
            p++;
            if (*p == 'l') { is_longlong = 1; p++; }
            else is_long = 1;
        } else if (*p == 'h') {
            p++;
            if (*p == 'h') { is_char = 1; p++; }
            else is_short = 1;
        } else if (*p == 'z') {
            is_size = 1; p++;
        }

        /* --- conversion --- */
        if (*p == 'd' || *p == 'i') {
            long long val;
            if (is_longlong) val = va_arg(ap, long long);
            else if (is_long || is_size) val = (long long)va_arg(ap, long);
            else val = (long long)va_arg(ap, int);

            char buf[64]; int bi = 0;
            int neg = 0;
            unsigned long long uval;
            if (val < 0) { neg = 1; uval = (unsigned long long)(-val); }
            else uval = (unsigned long long)val;

            if (uval == 0) buf[bi++] = '0';
            else { while (uval > 0) { buf[bi++] = '0' + (uval % 10); uval /= 10; } }

            /* apply precision: minimum digits */
            int min_digits = has_prec ? prec : 1;
            while (bi < min_digits) buf[bi++] = '0';

            /* prefix */
            char prefix = 0;
            if (neg) prefix = '-';
            else if (flag_plus) prefix = '+';
            else if (flag_space) prefix = ' ';

            int total_len = bi + (prefix ? 1 : 0);
            char pad = (flag_zero && !flag_minus && !has_prec) ? '0' : ' ';

            /* right-justify: pad before */
            if (!flag_minus && pad == ' ') {
                for (int i = total_len; i < width; i++) VSNP_PUT(' ');
            }
            if (prefix) VSNP_PUT(prefix);
            if (!flag_minus && pad == '0') {
                for (int i = total_len; i < width; i++) VSNP_PUT('0');
            }
            /* digits in reverse */
            for (int i = bi - 1; i >= 0; i--) VSNP_PUT(buf[i]);
            /* left-justify pad */
            if (flag_minus) {
                for (int i = total_len; i < width; i++) VSNP_PUT(' ');
            }
        } else if (*p == 'u') {
            unsigned long long val;
            if (is_longlong) val = va_arg(ap, unsigned long long);
            else if (is_long || is_size) val = (unsigned long long)va_arg(ap, unsigned long);
            else val = (unsigned long long)va_arg(ap, unsigned int);

            char buf[64]; int bi = 0;
            if (val == 0) buf[bi++] = '0';
            else { while (val > 0) { buf[bi++] = '0' + (val % 10); val /= 10; } }

            int min_digits = has_prec ? prec : 1;
            while (bi < min_digits) buf[bi++] = '0';

            int total_len = bi;
            char pad = (flag_zero && !flag_minus && !has_prec) ? '0' : ' ';
            if (!flag_minus) { for (int i = total_len; i < width; i++) VSNP_PUT(pad); }
            for (int i = bi - 1; i >= 0; i--) VSNP_PUT(buf[i]);
            if (flag_minus) { for (int i = total_len; i < width; i++) VSNP_PUT(' '); }
        } else if (*p == 'x' || *p == 'X') {
            static const char lc[] = "0123456789abcdef";
            static const char uc[] = "0123456789ABCDEF";
            const char *digits = (*p == 'X') ? uc : lc;

            unsigned long long val;
            if (is_longlong) val = va_arg(ap, unsigned long long);
            else if (is_long || is_size) val = (unsigned long long)va_arg(ap, unsigned long);
            else val = (unsigned long long)va_arg(ap, unsigned int);

            char buf[64]; int bi = 0;
            if (val == 0) buf[bi++] = '0';
            else { while (val > 0) { buf[bi++] = digits[val % 16]; val /= 16; } }

            int min_digits = has_prec ? prec : 1;
            while (bi < min_digits) buf[bi++] = '0';

            int total_len = bi;
            char pad = (flag_zero && !flag_minus && !has_prec) ? '0' : ' ';
            if (!flag_minus) { for (int i = total_len; i < width; i++) VSNP_PUT(pad); }
            for (int i = bi - 1; i >= 0; i--) VSNP_PUT(buf[i]);
            if (flag_minus) { for (int i = total_len; i < width; i++) VSNP_PUT(' '); }
        } else if (*p == 'p') {
            unsigned long val = va_arg(ap, unsigned long);
            VSNP_PUT('0'); VSNP_PUT('x');
            static const char hx[] = "0123456789abcdef";
            char buf[64]; int bi = 0;
            if (val == 0) buf[bi++] = '0';
            else { while (val > 0) { buf[bi++] = hx[val % 16]; val /= 16; } }
            for (int i = bi - 1; i >= 0; i--) VSNP_PUT(buf[i]);
        } else if (*p == 's') {
            char *s = va_arg(ap, char *);
            if (!s) s = "(null)";
            int slen = (int)strlen(s);
            if (has_prec && prec < slen) slen = prec;
            if (!flag_minus) { for (int i = slen; i < width; i++) VSNP_PUT(' '); }
            for (int i = 0; i < slen; i++) VSNP_PUT(s[i]);
            if (flag_minus) { for (int i = slen; i < width; i++) VSNP_PUT(' '); }
        } else if (*p == 'c') {
            char ch = (char)va_arg(ap, int);
            if (!flag_minus) { for (int i = 1; i < width; i++) VSNP_PUT(' '); }
            VSNP_PUT(ch);
            if (flag_minus) { for (int i = 1; i < width; i++) VSNP_PUT(' '); }
        } else if (*p == '%') {
            VSNP_PUT('%');
        } else {
            /* unknown specifier, just output it */
            VSNP_PUT('%');
            VSNP_PUT(*p);
        }
        p++;
    }
    if ((size_t)count < size) *d = '\0';
    else if (size > 0) str[size - 1] = '\0';
    return count;

    #undef VSNP_PUT
}


int vprintf(const char *fmt, va_list ap) {
    return vfprintf(stdout, fmt, ap);
}

int snprintf(char *str, size_t size, const char *format, ...) {
    va_list ap;
    va_start(ap, format);
    int n = vsnprintf(str, size, format, ap);
    va_end(ap);
    return n;
}

int vsprintf(char *str, const char *format, va_list ap) {
    return vsnprintf(str, 65536, format, ap);
}

int sprintf(char *str, const char *format, ...) {
    va_list ap;
    va_start(ap, format);
    int n = vsprintf(str, format, ap);
    va_end(ap);
    return n;
}

int vfprintf(FILE *stream, const char *fmt, va_list ap) {
    char buf[4096];
    int n = vsnprintf(buf, sizeof(buf), fmt, ap);
    if (n > 0) {
        int to_write = n < (int)sizeof(buf) ? n : (int)sizeof(buf) - 1;
        write(stream->fd, buf, to_write);
    }
    return n;
}

int fprintf(FILE *stream, const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int n = vfprintf(stream, fmt, ap);
    va_end(ap);
    return n;
}

int printf(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int n = vfprintf(stdout, fmt, ap);
    va_end(ap);
    return n;
}

/* GCC Fortification Stubs */
void __printf_chk(int flag, const char *fmt, ...) {
    (void)flag;
    va_list ap;
    va_start(ap, fmt);
    vfprintf(stdout, fmt, ap);
    va_end(ap);
}

void __fprintf_chk(FILE *fp, int flag, const char *fmt, ...) {
    (void)flag;
    va_list ap;
    va_start(ap, fmt);
    vfprintf(fp, fmt, ap);
    va_end(ap);
}

void __vfprintf_chk(FILE *fp, int flag, const char *fmt, va_list ap) {
    (void)flag;
    vfprintf(fp, fmt, ap);
}

int __snprintf_chk(char *s, size_t n, int flag, size_t slen, const char *format, ...) {
    (void)flag; (void)slen;
    va_list ap;
    va_start(ap, format);
    int res = vsnprintf(s, n, format, ap);
    va_end(ap);
    return res;
}

int __vsnprintf_chk(char *s, size_t n, int flag, size_t slen, const char *format, va_list ap) {
    (void)flag; (void)slen;
    return vsnprintf(s, n, format, ap);
}

void *__memcpy_chk(void *dest, const void *src, size_t n, size_t destlen) {
    (void)destlen;
    return memcpy(dest, src, n);
}

void *__memset_chk(void *s, int c, size_t n, size_t destlen) {
    (void)destlen;
    return memset(s, c, n);
}

/* Final Stubs for DOOM */
int remove(const char *p) { return unlink(p); }
/* rename is now an extern from syscall.S */
int mkdir(const char *p, unsigned int m) { (void)p; (void)m; return -1; }
int system(const char *c) { (void)c; return -1; }
double strtod(const char *s, char **e) { (void)s; if(e)*e=(char*)s; return 0.0; }
int fflush(FILE *f) { (void)f; return 0; }
int putc(int c, FILE *f) { return fputc(c, f); }
int __isoc99_sscanf(const char *s, const char *fmt, ...) { (void)s; (void)fmt; return 0; }

/* CType Locs */
static const unsigned short _ctype_b_table[384] = { 0 };
const unsigned short **__ctype_b_loc(void) {
    static const unsigned short *ptr = &_ctype_b_table[128];
    return &ptr;
}

static const int _ctype_upper_table[384] = { 0 };
const int **__ctype_toupper_loc(void) {
    static const int *ptr = &_ctype_upper_table[128];
    return &ptr;
}

int *__errno_location(void) { static int _e; return &_e; }

int sscanf(const char *str, const char *format, ...) {
    va_list ap;
    va_start(ap, format);
    int count = 0;
    const char *f = format;
    const char *s = str;

    while (*f) {
        if (*f == ' ') {
            while (*s == ' ' || *s == '\t' || *s == '\n' || *s == '\r') s++;
            f++;
        } else if (*f == '%') {
            f++;
            if (*f == 'x') {
                unsigned int *res = va_arg(ap, unsigned int *);
                while (*s == ' ' || *s == '\t') s++;
                if (*s == '0' && (s[1] == 'x' || s[1] == 'X')) s += 2;
                unsigned int val = 0;
                int found = 0;
                while (1) {
                    int digit;
                    if (*s >= '0' && *s <= '9') digit = *s - '0';
                    else if (*s >= 'a' && *s <= 'f') digit = *s - 'a' + 10;
                    else if (*s >= 'A' && *s <= 'F') digit = *s - 'A' + 10;
                    else break;
                    val = val * 16 + digit;
                    s++;
                    found = 1;
                }
                if (found) {
                    *res = val;
                    count++;
                } else break;
                f++;
            } else {
                break;
            }
        } else {
            if (*f == *s) { f++; s++; }
            else break;
        }
    }

    va_end(ap);
    return count;
}
