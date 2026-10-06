#!/bin/sh
# the share picker's link gone with ostrov: removed (deb: remove, purge; rpm: 0 left; arch: always), not upgraded
case "$1" in
	remove|purge|0|"") rm -f /usr/bin/ostrov-share-picker ;;
esac
