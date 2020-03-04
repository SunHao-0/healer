//
// Created by sam on 2020/2/26.
//
#include <fcntl.h>
#include "../interpreter.h"

#ifndef BUILTIN_MINI_STDLIB

void FcntlCreat(struct ParseState *Parser, struct Value *ReturnValue, struct Value **Param, int NumArgs) {
    ReturnValue->Val->Integer = creat(Param[0]->Val->Pointer, Param[1]->Val->UnsignedInteger);
}

void FcntlFcntl(struct ParseState *Parser, struct Value *ReturnValue, struct Value **Param, int NumArgs) {
    ReturnValue->Val->Integer = fcntl(Param[0]->Val->Integer, Param[1]->Val->Integer, Param[2]->Val->Integer);
}

void FcntlOpen(struct ParseState *Parser, struct Value *ReturnValue, struct Value **Param, int NumArgs) {

    ReturnValue->Val->Integer = open(Param[0]->Val->Pointer, Param[1]->Val->Integer,
                                     Param[2]->Val->UnsignedInteger);
}

void FcntlOpenAt(struct ParseState *Parser, struct Value *ReturnValue, struct Value **Param, int NumArgs) {

    ReturnValue->Val->Integer = openat(Param[0]->Val->Integer, Param[1]->Val->Pointer, Param[2]->Val->Integer,
                                       Param[3]->Val->UnsignedInteger);
}

//const char FcntlDefs[] = "typedef unsigned int mode_t;";

struct LibraryFunction FcntlFunctions[] = {
        {FcntlCreat,  "int  creat(char *, mode_t);"},
        {FcntlFcntl,  "int  fcntl(int, int, int);"},
        {FcntlOpen,   "int  open(char *, int, mode_t);"},
        {FcntlOpenAt, "int openat(int, char*, int, mode_t);"},
        {NULL, NULL}
};

void FcntlSetupFunc(Picoc *pc) {

}

#endif