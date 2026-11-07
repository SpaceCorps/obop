# The environment for building the Mileage app for iOS on the Mac (docs/spec/mobile.md 9.2).
# Source it in the Terminal that makes the build; it changes the shell only, and no file but the
# private folder named at the end:
#
#   . scripts/ios/env.sh        # from web/apps/mileage-app
#
# It puts the per-user CocoaPods and Vite+ on the PATH, sets the UTF-8 locale that CocoaPods needs,
# and keeps the caches of the tools in one folder. OBOP_TOOLS is where the port session installed
# them (default ~/build/tools: the Ruby gems of CocoaPods in gems/, the caches next to them); set it
# before sourcing to use another folder. It sets no OBOP_ variable that changes the app: the build
# variables of 9.2 (OBOP_ALLOW_CLEARTEXT and the others) are exported by hand, on purpose.

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
export CP_HOME_DIR="$OBOP_TOOLS/cocoapods-home"
# pnpm 12.6.0 reads its settings from variables named pnpm_config_<name>; npm_config_store_dir and
# PNPM_STORE_DIR are ignored by it (on the Mac, 2026-10-30: `pnpm store path` and `vp install` used
# ~/Library/pnpm/store/v11 with them and $OBOP_TOOLS/pnpm-store/v11 with this one).
export pnpm_config_store_dir="$OBOP_TOOLS/pnpm-store"
export __UNSAFE_EXPO_HOME_DIRECTORY="$OBOP_TOOLS/expo-home"

# CI must NOT be set. Expo reads CI=1 as "no person is here" and then refuses the question it asks
# before it builds for a phone ("Select a device"): `vp run ios:run:device` stops there. An
# environment file that sets CI=1 (the port session's own ~/build/mileage-ios-env.sh did) is why
# this line is here.
unset CI

# Where the Simulator drive (scripts/simulator/ui/drive.mjs) keeps its project, build, logs and
# screenshots. The drive hands the test account's password to the test runner through the
# environment, and xcodebuild may keep the environment in its results: so the folder is private
# (mode 700), and its `uitest` folder is deleted when the drive is done.
export OBOP_BUILD_ROOT="${OBOP_BUILD_ROOT:-$HOME/.obop-mileage-build}"
mkdir -p "$OBOP_BUILD_ROOT" && chmod 700 "$OBOP_BUILD_ROOT"
