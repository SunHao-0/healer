#include "../interpreter.h"

#ifndef BUILTIN_MINI_STDLIB




void StdintSetupFunc(Picoc *pc)
{
}



const char StdintDefs [] = " typedef signed char __int8_t; \
typedef unsigned char __uint8_t; \
typedef signed short __int16_t; \
typedef unsigned short __uint16_t; \
typedef signed int __int32_t; \
typedef unsigned int __uint32_t; \
typedef signed long __int64_t; \
typedef unsigned long __uint64_t; \
typedef __int8_t __int_least8_t; \
typedef __uint8_t __uint_least8_t; \
typedef __int16_t __int_least16_t; \
typedef __uint16_t __uint_least16_t; \
typedef __int32_t __int_least32_t; \
typedef __uint32_t __uint_least32_t; \
typedef __int64_t __int_least64_t; \
typedef __uint64_t __uint_least64_t; \
typedef long __intptr_t; \
typedef __int8_t int8_t; \
typedef __int16_t int16_t; \
typedef __int32_t int32_t; \
typedef __int64_t int64_t; \
typedef __uint8_t uint8_t; \
typedef __uint16_t uint16_t; \
typedef __uint32_t uint32_t; \
typedef __uint64_t uint64_t; \
typedef __int_least8_t int_least8_t; \
typedef __int_least16_t int_least16_t; \
typedef __int_least32_t int_least32_t; \
typedef __int_least64_t int_least64_t; \
typedef __uint_least8_t uint_least8_t; \
typedef __uint_least16_t uint_least16_t; \
typedef __uint_least32_t uint_least32_t; \
typedef __uint_least64_t uint_least64_t; \
typedef signed char int_fast8_t; \
typedef long int_fast16_t; \
typedef long int_fast32_t; \
typedef long int_fast64_t; \
typedef unsigned char uint_fast8_t; \
typedef unsigned long uint_fast16_t; \
typedef unsigned long uint_fast32_t; \
typedef unsigned long uint_fast64_t; \
typedef long intptr_t; \
typedef unsigned long uintptr_t; \
typedef struct A { int32_t a;};\
";

#endif
