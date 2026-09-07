#import <Foundation/Foundation.h>
#import <UserNotifications/UserNotifications.h>

// Identity probe only: never requests permission or posts an OS notification.
int main(int argc, const char *argv[]) {
    @autoreleasepool {
        NSBundle *bundle = [NSBundle mainBundle];
        if (!bundle.bundleIdentifier || ![bundle.bundlePath hasSuffix:@".app"]) return 1;
        UNUserNotificationCenter *center = [UNUserNotificationCenter currentNotificationCenter];
        if (!center) return 2;
        printf("%s\n%s\n%s\n", bundle.bundleIdentifier.UTF8String,
               [[bundle objectForInfoDictionaryKey:@"CFBundleDisplayName"] UTF8String],
               argc > 1 ? argv[1] : "");
    }
    return 0;
}
