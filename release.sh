set -x

PATH=/usr/local/bin:$PATH

rm -rf dist
git push origin master
python3 setup.py develop sdist bdist_wheel

twine upload -r pypi dist/*
