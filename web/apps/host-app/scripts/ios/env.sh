# The environment for building the host app for the iOS Simulator on the Mac (docs/spec/mobile-host.md
# 12.8). Source it in the shell that makes the build; it changes the shell only, and no file but the
# build folder named below:
#
#   . scripts/ios/env.sh        # from web/apps/host-app
#
# It puts the per-user CocoaPods and Vite+ on the PATH and sets the UTF-8 locale that CocoaPods
# needs. CocoaPods is used where it is installed (OBOP_TOOLS, default ~/build/tools: the Ruby gems in
# gems/) and nothing is written there: every cache of the tools (CocoaPods, pnpm, Expo) goes into
# the host's own folder OBOP_HOST_IOS_ROOT (default ~/build/obop-host-ios), which is made private
# (mode 700) because the Simulator drive keeps the test account's password in its results there.
#
# It sets no OBOP_ variable that changes the app: the build variables (OBOP_ALLOW_CLEARTEXT,
# OBOP_WEBVIEW_DEBUG, OBOP_DEFAULT_SERVER, OBOP_IOS_BUNDLE_ID) are exported by hand, on purpose.

OBOP_TOOLS="${OBOP_TOOLS:-$HOME/build/tools}"
export GEM_HOME="$OBOP_TOOLS/gems"
export GEM_PATH="$OBOP_TOOLS/gems"
export PATH="$OBOP_TOOLS/gems/bin:$PATH"
export LANG=en_US.UTF-8
export LC_ALL=en_US.UTF-8
export EXPO_NO_TELEMETRY=1
export COCOAPODS_DISABLE_STATS=1
# Vite+ (`vp`) and its Node.
[ -f "$HOME/.config/vite-plus/env" ] && . "$HOME/.config/vite-plus/env"

export OBOP_HOST_IOS_ROOT="${OBOP_HOST_IOS_ROOT:-$HOME/build/obop-host-ios}"
mkdir -p "$OBOP_HOST_IOS_ROOT/cache" && chmod 700 "$OBOP_HOST_IOS_ROOT"
export CP_HOME_DIR="$OBOP_HOST_IOS_ROOT/cache/cocoapods-home"
export CP_CACHE_DIR="$OBOP_HOST_IOS_ROOT/cache/cocoapods"
# pnpm 12 reads its settings from variables named pnpm_config_<name> (docs/spec/mobile.md 9.2).
export pnpm_config_store_dir="$OBOP_HOST_IOS_ROOT/cache/pnpm-store"
export __UNSAFE_EXPO_HOME_DIRECTORY="$OBOP_HOST_IOS_ROOT/cache/expo-home"
# The Node and the pnpm that the tree pins (.node-version, packageManager) are fetched by `vp` into
# VP_HOME: this keeps them in the host's folder too, next to the vp installed for the user.
export VP_HOME="$OBOP_HOST_IOS_ROOT/cache/vp-home"
