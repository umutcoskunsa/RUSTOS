#ifndef _LIMITS_H
#define _LIMITS_H

#define CHAR_BIT 8
#define SCHAR_MIN (-128)
#define SCHAR_MAX 127
#define UCHAR_MAX 255

#ifndef SHRT_MIN
#define SHRT_MIN (-32768)
#endif
#ifndef SHRT_MAX
#define SHRT_MAX 32767
#endif
#ifndef USHRT_MAX
#define USHRT_MAX 65535
#endif

#ifndef INT_MIN
#define INT_MIN (-2147483647 - 1)
#endif
#ifndef INT_MAX
#define INT_MAX 2147483647
#endif
#ifndef UINT_MAX
#define UINT_MAX 4294967295U
#endif

#ifndef LONG_MIN
#define LONG_MIN (-9223372036854775807L - 1)
#endif
#ifndef LONG_MAX
#define LONG_MAX 9223372036854775807L
#endif
#ifndef ULONG_MAX
#define ULONG_MAX 18446744073709551615UL
#endif
#endif
