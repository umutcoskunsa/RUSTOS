#ifndef _STDINT_H
#define _STDINT_H

typedef signed char        int8_t;
typedef signed short       int16_t;
typedef signed int         int32_t;
typedef signed long long   int64_t;

typedef unsigned char      uint8_t;
typedef unsigned short     uint16_t;
typedef unsigned int       uint32_t;
typedef unsigned long long uint64_t;

typedef int64_t  intptr_t;
typedef uint64_t uintptr_t;
typedef int64_t  intmax_t;
typedef uint64_t uintmax_t;

#define INT8_MIN    (-128)
#define INT16_MIN   (-32768)
#define INT32_MIN   (-2147483648)
#define INT64_MIN   (-9223372036854775807LL - 1)

#define INT8_MAX    127
#define INT16_MAX   32767
#define INT32_MAX   2147483647
#define INT64_MAX   9223372036854775807LL

#define UINT8_MAX   255U
#define UINT16_MAX  65535U
#define UINT32_MAX  4294967295U
#define UINT64_MAX  18446744073709551615ULL

#define INT_MAX     INT32_MAX
#define INT_MIN     INT32_MIN
#define LONG_MAX    INT64_MAX
#define LONG_MIN    INT64_MIN
#define ULONG_MAX   UINT64_MAX

#endif /* _STDINT_H */
