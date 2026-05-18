/*
 * stdlib.c — Utility functions for MYNEWOS libc
 */

#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <errno.h>

int errno = 0;

extern void _exit(int status) __attribute__((noreturn));

void exit(int status) {
    printf("libc: exit(%d) called\n", status);
    _exit(status);
}

void abort(void) {
    exit(1);
}

int abs(int x) {
    return x < 0 ? -x : x;
}

long labs(long x) {
    return x < 0 ? -x : x;
}

int atoi(const char *s) {
    int res = 0;
    int sign = 1;
    if (*s == '-') {
        sign = -1;
        s++;
    }
    while (*s >= '0' && *s <= '9') {
        res = res * 10 + (*s - '0');
        s++;
    }
    return res * sign;
}

double atof(const char *s) {
    double res = 0.0;
    double sign = 1.0;
    while (*s == ' ') s++;
    if (*s == '-') {
        sign = -1.0;
        s++;
    }
    while (*s >= '0' && *s <= '9') {
        res = res * 10.0 + (*s - '0');
        s++;
    }
    if (*s == '.') {
        s++;
        double div = 10.0;
        while (*s >= '0' && *s <= '9') {
            res += (*s - '0') / div;
            div *= 10.0;
            s++;
        }
    }
    return res * sign;
}

static unsigned long next = 1;

int rand(void) {
    next = next * 1103515245 + 12345;
    return (unsigned int)(next / 65536) % 32768;
}

void srand(unsigned int seed) {
    next = seed;
}

char *getenv(const char *name) {
    // For now we don't have environment variables
    (void)name;
    return NULL;
}

long strtol(const char *s, char **end, int base) {
    long res = 0;
    int sign = 1;
    while (*s == ' ' || *s == '\t') s++;
    if (*s == '-') {
        sign = -1;
        s++;
    } else if (*s == '+') {
        s++;
    }
    
    if (base == 0) {
        if (*s == '0') {
            if (s[1] == 'x' || s[1] == 'X') {
                base = 16;
                s += 2;
            } else {
                base = 8;
                s++;
            }
        } else {
            base = 10;
        }
    } else if (base == 16) {
        if (*s == '0' && (s[1] == 'x' || s[1] == 'X')) s += 2;
    }

    while (1) {
        int val;
        if (*s >= '0' && *s <= '9') val = *s - '0';
        else if (*s >= 'a' && *s <= 'z') val = *s - 'a' + 10;
        else if (*s >= 'A' && *s <= 'Z') val = *s - 'A' + 10;
        else break;
        
        if (val >= base) break;
        res = res * base + val;
        s++;
    }
    
    if (end) *end = (char *)s;
    return res * sign;
}

unsigned long strtoul(const char *s, char **end, int base) {
    return (unsigned long)strtol(s, end, base);
}
