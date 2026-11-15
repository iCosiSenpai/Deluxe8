// SPDX-License-Identifier: GPL-3.0-or-later
// MSVC runtime pieces that NVIDIA's static NGX library (nvsdk_ngx_d.lib, built with MSVC /MD) expects, so the DLSS plugin can be
// linked by the llvm-mingw toolchain that ships with the native engine (no Visual Studio needed on the player's PC).
// Everything the library imports from DLLs (UCRT, kernel32, vcruntime140's C++ EH) is linked normally; this file only provides
// the parts that live in MSVC's static libraries (operator new/delete, thread-safe statics, /GS handlers) and the two
// msvcp140 failure helpers.
#include <limits.h>
#include <stdint.h>
#include <stdlib.h>

#define WIN32_LEAN_AND_MEAN
#include <windows.h>

extern "C" {

// ---- operator new / delete with MSVC names: void* __cdecl operator new(size_t), void __cdecl operator delete(void*)
void* d8_msvc_new(size_t size) __asm__("??2@YAPEAX_K@Z");
void* d8_msvc_new(size_t size) {
    void* p = malloc(size ? size : 1);
    if (!p) abort();
    return p;
}
void d8_msvc_delete(void* p) __asm__("??3@YAXPEAX@Z");
void d8_msvc_delete(void* p) {
    free(p);
}

// ---- msvcp140 failure helpers, reached through their import slots (__imp_...): out of memory / impossible lengths only
static void d8_xbad_alloc() {
    abort();
}
static void d8_xlength_error(const char*) {
    abort();
}
void (*d8_imp_xbad_alloc)() __asm__("__imp_?_Xbad_alloc@std@@YAXXZ") = d8_xbad_alloc;
void (*d8_imp_xlength_error)(const char*) __asm__("__imp_?_Xlength_error@std@@YAXPEBD@Z") = d8_xlength_error;

// ---- thread-safe local statics (MSVC's protocol: guard 0 = uninitialized, -1 = being initialized, else the epoch it finished in)
__thread int _Init_thread_epoch = INT_MIN;
static int g_init_epoch = INT_MIN;
static SRWLOCK g_init_lock = SRWLOCK_INIT;
static CONDITION_VARIABLE g_init_cv = CONDITION_VARIABLE_INIT;

void _Init_thread_header(int* once) {
    AcquireSRWLockExclusive(&g_init_lock);
    if (*once == 0) {
        *once = -1; // this thread runs the initializer
    } else {
        while (*once == -1) {
            SleepConditionVariableSRW(&g_init_cv, &g_init_lock, INFINITE, 0);
        }
        _Init_thread_epoch = g_init_epoch;
    }
    ReleaseSRWLockExclusive(&g_init_lock);
}

void _Init_thread_footer(int* once) {
    AcquireSRWLockExclusive(&g_init_lock);
    ++g_init_epoch;
    *once = g_init_epoch;
    _Init_thread_epoch = g_init_epoch;
    ReleaseSRWLockExclusive(&g_init_lock);
    WakeAllConditionVariable(&g_init_cv);
}

void _Init_thread_abort(int* once) {
    AcquireSRWLockExclusive(&g_init_lock);
    *once = 0;
    ReleaseSRWLockExclusive(&g_init_lock);
    WakeAllConditionVariable(&g_init_cv);
}

// ---- /GS exception filters: the cookie check is skipped, C++ frames go to vcruntime140's handler
EXCEPTION_DISPOSITION __CxxFrameHandler3(PEXCEPTION_RECORD, void*, PCONTEXT, PDISPATCHER_CONTEXT);
EXCEPTION_DISPOSITION __GSHandlerCheck(PEXCEPTION_RECORD, void*, PCONTEXT, PDISPATCHER_CONTEXT) {
    return ExceptionContinueSearch;
}
EXCEPTION_DISPOSITION __GSHandlerCheck_EH(PEXCEPTION_RECORD record, void* frame, PCONTEXT context, PDISPATCHER_CONTEXT dispatch) {
    return __CxxFrameHandler3(record, frame, context, dispatch);
}

// ---- /GS stack cookies. The value is randomised when the DLL loads, before any NGX code runs; the check is written in assembly
// so it keeps RAX/XMM0 (the caller's return value) untouched, like MSVC's own.
uintptr_t __security_cookie = 0x00002B992DDFA232ULL;
void d8_security_cookie_failed() {
    abort();
}
__attribute__((naked)) void __security_check_cookie(uintptr_t) {
    __asm__ volatile(
        "cmpq __security_cookie(%rip), %rcx\n\t"
        "jne 1f\n\t"
        "ret\n"
        "1:\n\t"
        "jmp d8_security_cookie_failed\n\t");
}
__attribute__((constructor)) static void d8_init_security_cookie() {
    LARGE_INTEGER counter;
    QueryPerformanceCounter(&counter);
    uintptr_t cookie = static_cast<uintptr_t>(counter.QuadPart) ^ static_cast<uintptr_t>(GetTickCount64() << 24) ^
                       reinterpret_cast<uintptr_t>(&counter) ^ (static_cast<uintptr_t>(GetCurrentThreadId()) << 32);
    cookie &= 0x0000FFFFFFFFFFFFULL; // MSVC keeps the top 16 bits clear
    if (cookie == 0 || cookie == 0x00002B992DDFA232ULL) cookie = 0x00002B992DDFA233ULL;
    __security_cookie = cookie;
}

// ---- marks that the module uses floating point (MSVC's linker supplies it)
int _fltused = 0x9875;

// (__guard_check_icall_fptr, the Control Flow Guard hook, comes from mingw-w64's own CRT.)

} // extern "C"
