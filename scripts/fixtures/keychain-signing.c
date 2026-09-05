#include <CoreFoundation/CoreFoundation.h>
#include <Security/Security.h>
#include <stdio.h>
#include <string.h>

#ifndef JARVIS_BUILD
#define JARVIS_BUILD 1
#endif

int main(int argc, char **argv) {
    if (argc != 3) return 2;
    // Any attempt to prompt is a test failure, including reads by an unrelated signer.
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    OSStatus status = SecKeychainSetUserInteractionAllowed(false);
#pragma clang diagnostic pop
    if (status != errSecSuccess) return 3;

    CFStringRef service = CFStringCreateWithCString(NULL, argv[2], kCFStringEncodingUTF8);
    const void *keys[] = { kSecClass, kSecAttrService, kSecAttrAccount };
    const void *values[] = { kSecClassGenericPassword, service, CFSTR("signing-check") };
    CFMutableDictionaryRef query = CFDictionaryCreateMutable(NULL, 0, &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
    for (unsigned int index = 0; index < 3; index++) CFDictionarySetValue(query, keys[index], values[index]);

    const char *value = "disposable-jarvis-signing-check";
    if (strcmp(argv[1], "write") == 0) {
        CFDataRef data = CFDataCreate(NULL, (const UInt8 *)value, (CFIndex)strlen(value));
        CFDictionarySetValue(query, kSecValueData, data);
        status = SecItemAdd(query, NULL);
        CFRelease(data);
    } else if (strcmp(argv[1], "read") == 0) {
        CFDictionarySetValue(query, kSecReturnData, kCFBooleanTrue);
        CFTypeRef result = NULL;
        status = SecItemCopyMatching(query, &result);
        if (status == errSecSuccess) {
            if (!result || CFGetTypeID(result) != CFDataGetTypeID()
                || CFDataGetLength((CFDataRef)result) != (CFIndex)strlen(value)
                || memcmp(CFDataGetBytePtr((CFDataRef)result), value, strlen(value)) != 0) status = errSecDecode;
        }
        if (result) CFRelease(result);
    } else if (strcmp(argv[1], "delete") == 0) {
        status = SecItemDelete(query);
        if (status == errSecItemNotFound) status = errSecSuccess;
    } else {
        status = errSecParam;
    }
    CFRelease(query);
    CFRelease(service);
    printf("build=%d operation=%s status=%d\n", JARVIS_BUILD, argv[1], (int)status);
    return status == errSecSuccess ? 0 : 1;
}
