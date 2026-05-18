/*
 * string.c — String and memory functions for MYNEWOS libc
 */

#include <string.h>
#include <stdlib.h>

/* -------------------------------------------------------------------------- */
/* Memory functions                                                             */
/* -------------------------------------------------------------------------- */
void *memcpy(void *dest, const void *src, size_t n) {
    char       *d = (char *)dest;
    const char *s = (const char *)src;
    for (size_t i = 0; i < n; i++) d[i] = s[i];
    return dest;
}

void *memmove(void *dest, const void *src, size_t n) {
    char       *d = (char *)dest;
    const char *s = (const char *)src;
    if (d < s) {
        for (size_t i = 0; i < n; i++) d[i] = s[i];
    } else if (d > s) {
        for (size_t i = n; i-- > 0; ) d[i] = s[i];
    }
    return dest;
}

void *memset(void *s, int c, size_t n) {
    unsigned char *p = (unsigned char *)s;
    for (size_t i = 0; i < n; i++) p[i] = (unsigned char)c;
    return s;
}

int memcmp(const void *s1, const void *s2, size_t n) {
    const unsigned char *a = (const unsigned char *)s1;
    const unsigned char *b = (const unsigned char *)s2;
    for (size_t i = 0; i < n; i++) {
        if (a[i] != b[i]) return (int)a[i] - (int)b[i];
    }
    return 0;
}

void *memchr(const void *s, int c, size_t n) {
    const unsigned char *p = (const unsigned char *)s;
    for (size_t i = 0; i < n; i++) {
        if (p[i] == (unsigned char)c) return (void *)(p + i);
    }
    return (void *)0;
}

/* -------------------------------------------------------------------------- */
/* String functions                                                             */
/* -------------------------------------------------------------------------- */
size_t strlen(const char *s) {
    size_t n = 0;
    while (s[n]) n++;
    return n;
}

char *strcpy(char *dest, const char *src) {
    char *d = dest;
    while ((*d++ = *src++));
    return dest;
}

char *strncpy(char *dest, const char *src, size_t n) {
    size_t i;
    for (i = 0; i < n && src[i]; i++) dest[i] = src[i];
    for (; i < n; i++) dest[i] = '\0';
    return dest;
}

char *strcat(char *dest, const char *src) {
    char *d = dest + strlen(dest);
    while ((*d++ = *src++));
    return dest;
}

char *strncat(char *dest, const char *src, size_t n) {
    char *d = dest + strlen(dest);
    size_t i;
    for (i = 0; i < n && src[i]; i++) d[i] = src[i];
    d[i] = '\0';
    return dest;
}

int strcmp(const char *s1, const char *s2) {
    while (*s1 && (*s1 == *s2)) { s1++; s2++; }
    return (unsigned char)*s1 - (unsigned char)*s2;
}

int strncmp(const char *s1, const char *s2, size_t n) {
    for (size_t i = 0; i < n; i++) {
        if (s1[i] != s2[i]) return (unsigned char)s1[i] - (unsigned char)s2[i];
        if (!s1[i]) return 0;
    }
    return 0;
}

char *strchr(const char *s, int c) {
    while (*s) {
        if (*s == (char)c) return (char *)s;
        s++;
    }
    return (c == '\0') ? (char *)s : (char *)0;
}

char *strrchr(const char *s, int c) {
    const char *last = (char *)0;
    while (*s) {
        if (*s == (char)c) last = s;
        s++;
    }
    if (c == '\0') return (char *)s;
    return (char *)last;
}

char *strstr(const char *haystack, const char *needle) {
    if (!*needle) return (char *)haystack;
    size_t nl = strlen(needle);
    for (; *haystack; haystack++) {
        if (memcmp(haystack, needle, nl) == 0) return (char *)haystack;
    }
    return (char *)0;
}

static char *strtok_save;
char *strtok(char *str, const char *delim) {
    if (str) strtok_save = str;
    if (!strtok_save) return (char *)0;

    /* Skip leading delimiters */
    char *p = strtok_save;
    while (*p && strchr(delim, *p)) p++;
    if (!*p) { strtok_save = (char *)0; return (char *)0; }

    char *tok = p;
    while (*p && !strchr(delim, *p)) p++;
    if (*p) { *p = '\0'; strtok_save = p + 1; }
    else    { strtok_save = (char *)0; }
    return tok;
}

char *strdup(const char *s) {
    size_t n = strlen(s) + 1;
    char *copy = (char *)malloc(n);
    if (copy) memcpy(copy, s, n);
    return copy;
}

char *__strncpy_chk(char *dest, const char *src, size_t n, size_t destlen) {
    (void)destlen;
    return strncpy(dest, src, n);
}

static inline int _tolower(int c) {
    return (c >= 'A' && c <= 'Z') ? (c - 'A' + 'a') : c;
}

int strcasecmp(const char *s1, const char *s2) {
    while (*s1 && (_tolower((unsigned char)*s1) == _tolower((unsigned char)*s2))) {
        s1++; s2++;
    }
    return _tolower((unsigned char)*s1) - _tolower((unsigned char)*s2);
}

int strncasecmp(const char *s1, const char *s2, size_t n) {
    for (size_t i = 0; i < n; i++) {
        int c1 = _tolower((unsigned char)s1[i]);
        int c2 = _tolower((unsigned char)s2[i]);
        if (c1 != c2) return c1 - c2;
        if (!c1) return 0;
    }
    return 0;
}
