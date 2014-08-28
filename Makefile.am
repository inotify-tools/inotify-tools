ACLOCAL_AMFLAGS = -I m4

SUBDIRS = libinotifytools src man

dist-hook:
	git log --pretty=format:'%s' > $(distdir)/ChangeLog
	touch -d '2 days ago' $(distdir)/*
