Unsigned pre-release build. See CHANGELOG.md for what is included and the known gaps.

**macOS:** the app is not notarized yet, so the first launch needs one extra step. Drag Alloy to Applications, then right-click it and choose Open, or allow it under System Settings > Privacy & Security > Open Anyway. From a terminal: `xattr -dr com.apple.quarantine /Applications/Alloy.app`

**Windows:** SmartScreen may warn; choose More info > Run anyway.

**Android:** signed with a throwaway key; uninstall the previous version before installing a new one.
