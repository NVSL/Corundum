#ifndef __PVAR_H__
#define __PVAR_H__

#include <stdio.h>
#include <mnemosyne.h>
#include <mtm.h>
#include <assert.h>

#define TM_SAFE __attribute__((transaction_safe))
#define TM_CALL __attribute__((transaction_callable))
#define TM_PURE __attribute__((transaction_pure))
#define TM_ATTR TM_SAFE

#define TM_ATOMIC	__transaction_atomic
#define TM_RELAXED	__transaction_relaxed
#define PTx		TM_RELAXED

#ifdef __PVAR_C__

#define PVAR(type, var) \
        MNEMOSYNE_PERSISTENT type var;  		\
        TM_ATTR type pset_##var(type __##var) 		\
			{ return (var = __##var); }	\
        TM_ATTR type pget_##var() { return var; }	\
        TM_ATTR void* paddr_##var() { return (void*)&var; }

#else /* !__PVAR_C__ */

#define PVAR(type, var) \
        extern MNEMOSYNE_PERSISTENT type var;  	\
        extern TM_ATTR type pset_##var(type);   \
        extern TM_ATTR type pget_##var();       \
        extern TM_ATTR void* paddr_##var();

#endif /* __PVAR_C__ */

#define PSET(var, val)  \
        pset_##var(val)
#define PGET(var)       \
        pget_##var()
#define PADDR(var)      \
        paddr_##var()

typedef struct node_t {
    int64_t key;
    char value[32];
    struct node_t *slots[2];
} node_t;

PVAR(node_t*, root);

#endif /* __PVAR_H__ */