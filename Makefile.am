
SUBDIRS = libinotifytools src man

dist-hook:
	svn2cl -r HEAD:1 --stdout --break-before-msg . | sed -e 's|ro_han|Rohan McGovern|g' > $(distdir)/ChangeLog
	touch -d '2 days ago' $(distdir)/*
