from setuptools import setup
import os

here = os.path.abspath(os.path.dirname(__file__))
README = open(os.path.join(here, 'README.rst')).read()
#NEWS = open(os.path.join(here, 'NEWS.txt')).read()

rootdir = os.path.dirname(os.path.abspath(__file__))
exec(open(rootdir + '/cerridwen/version.py').read())
version = __VERSION__

setup(name='cerridwen',
      version=version,
      description='Accurate solar system data for everyone',
      long_description=README,
      author='Leslie P. Polzer',
      author_email='polzer@fastmail.com',
      url='http://cerridwen.bluemagician.vc/',
      license='MIT',
      classifiers=[
        # Get strings from http://pypi.python.org/pypi?%3Aaction=list_classifiers
        "Development Status :: 4 - Beta"
      , "Environment :: Console"
      , "Intended Audience :: Science/Research"
      , "Intended Audience :: Developers"
      , "License :: OSI Approved :: MIT License"
      , "Operating System :: OS Independent"
      , "Programming Language :: Python :: 3"
      , "Topic :: Scientific/Engineering :: Astronomy"
      , "Topic :: Other/Nonlisted Topic"
      , "Topic :: Software Development :: Libraries :: Python Modules"
      , "Topic :: Utilities"
      ],
      maintainer='Leslie P. Polzer',
      maintainer_email='polzer@fastmail.com',
      packages=['cerridwen'],
      requires=['pyswisseph', 'numpy', 'astropy(>=0.4)'],
      extras_require={'Flask':['flask']},
      entry_points={
          'console_scripts':
              ['cerridwen = cerridwen.cli:main',
               'cerridwen-server = cerridwen.api_server:main [Flask]']
      })

