# FOTS 
Declaration language for interface used for interface aware fuzzing.

## basic type
Num: i8 i16 ... isize, usize

## type constructor 
#### Pointer
Grammar: *T, eg:*i8,*i16

#### Union
Grammar: union Ident{ name:Type(,name:Type)*}

#### Struct
Grammar: struct Ident{ name:Type(,name:Type)*}

####  Slice (Dynamic size)
Grammar: [T[;Len|Range]]

####  String (Dynamic size)
Grammar: str,cstr

#### Flag
Grammar: flag Ident<Num> {val(,val)*}

### Special Type Constructor 

#### Limit
Grammar: Num{NumLimit}|Str{StrLimit}|cstr{cstrLimit}|[T]{SliceLimit}
         NumLimit: [Range]|[Vals] 
                   Range: num:num
                   Vals: {num(,num)*}
         StrLimit: Vals
         
#### Res 
Grammar: Res<T>
         
### Special Type

#### Len
Grammar: Len<Type,Path>
         Path: ArgPath|FieldPath
         ArgPath: Ident
         FieldPath: Ident(.Ident)*

#### Path

### Function declaration 
Grammar: Ident()[Type] | Ident(ArgDec(,ArgDec)*)[Type]
         ArgDec: Ident:[[in|out|inout] ]PtrType | Ident:NorPrtType
         
### Func Spe
Ident@Ident ... 

### Rule
Grammar: Rule Ident { func ,func}


### Group 
Grammar: Group Ident{}


## Grammar

Fots: Item Fots| Item                                                       <br/>
Item => TypeDef | FuncDef | RuleDef | GroupDef                                <br/>
TypeDef => StructDef | UnionDef | FlagDef | TypeAlias                       <br/>
StructDef => struct Ident { Fields }                   <br/>
UnionDef => union Ident { Fields }                                            <br/>
Fields => Field , Fields | Field
Field  => Ident : TypeExp

FlagDef =>   flag Ident { Flags }                           <br/>
Flags   => Flag,Flags | Flag
Flag => Ident = NumLiteral
TypeAlias: type Ident = Type                                                <br/>
**Unstable** ConstDef: const Ident:NumType = Num                            <br/>
                       
FuncDef: Ident\[$Ident\](\[ParamDec(,ArgDec)*\])\[Type\]                           <br/>
         ArgDec: Ident:Type                                                  <br/>
FuncIdent: Ident\[$Ident\]                                                   <br/>
Type: (Type) | Ident | NumType\[{NumLimit}\] | \[in|out|inout] PtrType |                                                                      


GroupDef:group Ident { FuncDef(,FuncDef)*}                                   <br/>
TypeExp: \[TypeExp\[;Len|Range\]\] | *\[in|out|inout\] TypeExp | TypeItm
TypeItm: Ident | SpTypeItm
SpTypeItm: NumType{NumLimit} | StrType{StrLimit}
NumType:   i8| ... | usize | isize
StrType: cstr| str 
                       
RuleDef: rule Ident { CallExp}                                              <br/>
Exp:     rTplCall ; CallExp | Itm ; CallExp | Itm                       <br/>
Itm:    Fact \| Itm | Fact                                                  <br/>
Fact:   {Exp} |  {Exp}+ | TplCall                                          <br/>
TplCall:   FuncIdent(\[(Ident| StrLiteral | NumLiteral | PlaceHolder \])    <br/>
RTplCall:   Ident = TplCall
                    