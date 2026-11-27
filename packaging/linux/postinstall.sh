#!/bin/sh
# Load the udev rule that gives the logged-in user access to the machine.
udevadm control --reload-rules >/dev/null 2>&1 || true
udevadm trigger --subsystem-match=usb >/dev/null 2>&1 || true
