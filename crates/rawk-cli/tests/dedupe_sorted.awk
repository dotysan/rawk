#! /usr/bin/env -S awk -f
#
# Simple awk test to deduplicate sorted output on $1.
#

$1 != prev {
    print
    prev = $1
}
