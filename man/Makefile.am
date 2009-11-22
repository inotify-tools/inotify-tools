man_MANS = inotifywait.1 inotifywatch.1
EXTRA_DIST = $(man_MANS)

dist-hook:
    # Custom man page preprocessing!  Yippee!!!
	PACKAGE_VERSION=`sed -n -r -e 's|AC_INIT\(.+, \[(.+)\], .+\)|\1|p' \
	                 $(top_distdir)/configure.ac`; \
	DATE=`date +'%B %d, %Y'`; \
	for file in $(man_MANS); do \
	    sed -r -i -e "s|_PACKAGE_VERSION_|$${PACKAGE_VERSION}|g" \
	              -e "s|_DATE_|$${DATE}|g" $(distdir)/$${file}; \
	done
	touch -d '2 days ago' $(distdir)/*

