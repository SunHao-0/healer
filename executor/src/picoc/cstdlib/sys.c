//
// Created by sam on 2020/2/26.
//

#include "../interpreter.h"
#include <sys/mman.h>
#include <sys/stat.h>


#ifndef BUILTIN_MINI_STDLIB

void SysMmap(struct ParseState *Parser, struct Value *ReturnValue, struct Value **Param, int NumArgs) {
    ReturnValue->Val->Pointer = mmap(Param[0]->Val->Pointer, Param[1]->Val->Integer,
                                     Param[2]->Val->Integer, Param[3]->Val->Integer,
                                     Param[4]->Val->Integer, Param[5]->Val->Integer);
}

void SysMunmap(struct ParseState *Parser, struct Value *ReturnValue, struct Value **Param, int NumArgs) {
    ReturnValue->Val->Integer = munmap(Param[0]->Val->Pointer, Param[1]->Val->Integer);
}

void SysChmod(struct ParseState *Parser, struct Value *ReturnValue, struct Value **Param, int NumArgs) {
    ReturnValue->Val->Integer = chmod(Param[0]->Val->Pointer, Param[1]->Val->Integer);
}

void SysFChmod(struct ParseState *Parser, struct Value *ReturnValue, struct Value **Param, int NumArgs) {
    ReturnValue->Val->Integer = fchmod(Param[0]->Val->Integer, Param[1]->Val->Integer);
}

struct LibraryFunction SysFunctions[] = {
        {SysChmod,  "int chmod( char *pathname, mode_t mode);"},
        {SysFChmod, "int fchmod(int fd, mode_t mode);"},
        {SysMmap,   "void *mmap(void *, size_t, int , int , int , off_t);"},
            {SysMunmap, "int munmap(void *addr, size_t length);"},
        {NULL, NULL}
};

void SysSetupFunc(Picoc *pc) {

}

#endif