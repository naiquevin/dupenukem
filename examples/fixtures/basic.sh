set -e

mkdir foo bar cat
echo "ONE" > foo/1.txt
echo "TWO" > bar/2.txt
echo "BAR_ONE" > bar/1.txt
cp foo/1.txt cat/

# create a file in another dir
echo "EXTERNAL" > /tmp/xx.txt
ln -s /tmp/xx.txt foo/xx.txt

ln -s ../bar/1.txt cat/bar_one.txt
