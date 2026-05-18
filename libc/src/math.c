/*
 * math.c — Basic math functions for MYNEWOS libc
 */

#include <math.h>

double fabs(double x) {
    return x < 0 ? -x : x;
}

double floor(double x) {
    long i = (long)x;
    if (x < 0 && x != (double)i) i--;
    return (double)i;
}

double ceil(double x) {
    long i = (long)x;
    if (x > 0 && x != (double)i) i++;
    return (double)i;
}

/* Very simple power function for integers, could be improved */
double pow(double base, double exp) {
    if (exp == 0) return 1.0;
    if (exp == 1) return base;
    
    double res = 1.0;
    long e = (long)exp;
    for (long i = 0; i < e; i++) {
        res *= base;
    }
    return res;
}

/* 
 * These functions are much harder to implement from scratch without 
 * series approximations. For DOOM, we might need to use compiler builtins 
 * or more complex series if it relies heavily on floating point math.
 * DOOM actually uses fixed-point math for most things internally.
 */

double sqrt(double x) {
    return __builtin_sqrt(x);
}

double sin(double x) {
    return __builtin_sin(x);
}

double cos(double x) {
    return __builtin_cos(x);
}

double atan2(double y, double x) {
    return __builtin_atan2(y, x);
}

double log(double x) {
    return __builtin_log(x);
}
