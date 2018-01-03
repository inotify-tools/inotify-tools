#!/bin/sh
#
# Copyright (c) 2008-2012 Git project
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 2 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see http://www.gnu.org/licenses/ .

failed_tests=
fixed=0
success=0
failed=0
broken=0
total=0

while read file; do
	while read type value; do
		case $type in
		'')
			continue ;;
		fixed)
			fixed=$(($fixed + $value)) ;;
		success)
			success=$(($success + $value)) ;;
		failed)
			failed=$(($failed + $value))
			if test $value != 0; then
				test_name=$(expr "$file" : 'test-results/\(.*\)\.[0-9]*\.counts')
				failed_tests="$failed_tests $test_name"
			fi
			;;
		broken)
			broken=$(($broken + $value)) ;;
		total)
			total=$(($total + $value)) ;;
		esac
	done <"$file"
done

if test -n "$failed_tests"; then
	printf "\nfailed test(s):$failed_tests\n\n"
fi

printf "%-8s%d\n" fixed $fixed
printf "%-8s%d\n" success $success
printf "%-8s%d\n" failed $failed
printf "%-8s%d\n" broken $broken
printf "%-8s%d\n" total $total
