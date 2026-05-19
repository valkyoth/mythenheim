#!/usr/bin/env perl
use strict;
use warnings;

my @files = grep { -f $_ } @ARGV;
if (!@files) {
    chomp(@files = `find . -path ./target -prune -o -name '*.md' -print`);
}

my $failed = 0;
for my $file (@files) {
    open my $fh, '<', $file or die "cannot read $file: $!";
    while (my $line = <$fh>) {
        while ($line =~ /\[[^\]]+\]\(([^)]+)\)/g) {
            my $target = $1;
            next if $target =~ m{^[a-z][a-z0-9+.-]*:}i;
            next if $target =~ /^#/;
            $target =~ s/#.*$//;
            next if $target eq '';
            my $base = $file;
            $base =~ s{/[^/]+$}{};
            my $path = "$base/$target";
            if (!-e $path) {
                warn "$file links to missing path: $target\n";
                $failed = 1;
            }
        }
    }
}

exit $failed;
