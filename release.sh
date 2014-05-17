set -x

rm -rf dist
git push origin master
python setup.py develop sdist bdist_wheel
twine upload -r pypi
