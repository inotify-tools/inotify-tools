AC_LOCAL_AMFLAGS = -l m4

SUBDIRS = libinotifytools src man

dist-hook:
	svn2cl -r HEAD:1 --stdout --break-before-msg . | sed -e 's|ro_han|Rohan McGovern|g' -e 's|raduv|Radu Voicilas|g' > $(distdir)/ChangeLog
	touch -d '2 days ago' $(distdir)/*
