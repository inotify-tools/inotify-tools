ACLOCAL_AMFLAGS = -I m4

SUBDIRS = libinotifytools src man

README: README.md

dist-hook:
# Automake official documentation states that dist-hook should assume files
# are not writable.
	chmod u+w $(distdir)/ChangeLog
	git log --pretty=format:'%s' > $(distdir)/ChangeLog
