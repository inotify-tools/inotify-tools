#!/bin/sh

cd libinotifytools/src || exit 1
doxygen || exit 1
for file in doc/html/*.html; do
	sed -r -i -e 's|</body>|\
<!-- Google Analytics. -->\
<script src="http://www.google-analytics.com/urchin.js" type="text/javascript">\
</script>\
<script type="text/javascript">\
_uacct = "UA-514445-2";\
urchinTracker();\
</script>\
</body>|' "$file" || exit 1
done

scp doc/html/* ro_han@shell.sourceforge.net:/home/users/r/ro/ro_han/inotify-tools/htdocs/api || exit 1

