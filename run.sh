
#!/bin/bash

export PKG_CONFIG_PATH=/usr/lib/pkgconfig:/usr/share/pkgconfig

export PKG_CONFIG_LIBDIR=/usr/lib/pkgconfig:/usr/share/pkgconfig

export PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1

export PKG_CONFIG_x86_64_unknown_linux_gnu=/usr/bin/pkg-config

export HOST_PKG_CONFIG=/usr/bin/pkg-config

npm run tauri dev

